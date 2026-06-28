// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Scene compositor — the OBS-style layered program output.
//!
//! Composites a scene's layers (position, size, z-order, opacity) into a single
//! FLV stream via an ffmpeg filter graph, and publishes it to the scene's media
//! relay. The existing program router then forwards that composite exactly like
//! any other source, so fan-out, recording and the watch page all get the mix.
//!
//! Each layer's live source (already normalized to the output canvas by
//! `source_normalizer`/`srt`) is fed into ffmpeg over a local TCP stream; ffmpeg
//! scales + overlays them in z-order and re-encodes the composite.

use bytes::Bytes;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::routes::output::OutputConfig;
use crate::state::AppState;
use muxshed_common::LayerFit;

struct CompLayer {
    source_id: Uuid,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    opacity: f32,
    fit: LayerFit,
}

/// Translate a layer's fit mode into `(scale_chain, overlay_x, overlay_y)` for a
/// `w`x`h` box at `x`,`y`:
///   fill    — stretch to the box (ignores aspect)
///   cover   — scale to cover preserving aspect, then crop to the box
///   contain — scale to fit inside preserving aspect, then centre in the box
fn fit_filter(fit: LayerFit, x: i32, y: i32, w: u32, h: u32) -> (String, String, String) {
    match fit {
        LayerFit::Fill => (format!("scale={}:{}", w, h), x.to_string(), y.to_string()),
        LayerFit::Cover => (
            format!("scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}", w, h, w, h),
            x.to_string(),
            y.to_string(),
        ),
        LayerFit::Contain => (
            format!("scale={}:{}:force_original_aspect_ratio=decrease", w, h),
            // centre the (smaller) scaled image within the box; w/h here are the
            // overlay's post-scale dimensions, resolved by ffmpeg at runtime.
            format!("{}+({}-w)/2", x, w),
            format!("{}+({}-h)/2", y, h),
        ),
    }
}

/// Build the ffmpeg `filter_complex` that composites the layers (bottom-first)
/// onto a black canvas, and return `(graph, final_video_label)`.
fn build_filter_graph(layers: &[CompLayer], width: u32, height: u32, fps: u32) -> (String, String) {
    let mut graph = vec![format!("color=c=black:s={}x{}:r={}[base]", width, height, fps)];
    let mut prev = "base".to_string();
    for (i, layer) in layers.iter().enumerate() {
        let (scale, ox, oy) = fit_filter(layer.fit, layer.x, layer.y, layer.w, layer.h);
        // setpts=PTS-STARTPTS rebases each live source to a 0-based clock. Each
        // source has its own timestamp epoch (one running for minutes, a freshly
        // added image starting near zero); without rebasing, overlay's framesync
        // can't pair frames across layers and the graph stalls with no output.
        if layer.opacity < 0.999 {
            graph.push(format!(
                "[{}:v]setpts=PTS-STARTPTS,{},format=yuva420p,colorchannelmixer=aa={:.3}[l{}]",
                i, scale, layer.opacity, i
            ));
        } else {
            graph.push(format!("[{}:v]setpts=PTS-STARTPTS,{}[l{}]", i, scale, i));
        }
        let out = format!("s{}", i);
        graph.push(format!(
            "[{}][l{}]overlay=x={}:y={}:eof_action=pass[{}]",
            prev, i, ox, oy, out
        ));
        prev = out;
    }
    (graph.join(";"), prev)
}

/// Start (or restart) the compositor for a scene and publish to its media relay.
/// Only layers whose source is currently live are included.
pub async fn start_scene_compositor(state: Arc<AppState>, scene_id: Uuid) -> Result<(), String> {
    stop_scene_compositor(&state, &scene_id).await;

    let rows = sqlx::query_as::<_, (String, i64, i64, i64, i64, i64, f64, String)>(
        "SELECT source_id, x, y, width, height, z_index, opacity, fit \
         FROM scene_layers WHERE scene_id = ? ORDER BY z_index ASC",
    )
    .bind(scene_id.to_string())
    .fetch_all(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    let mut layers = Vec::new();
    for (sid, x, y, w, h, _z, op, fit) in rows {
        if let Ok(source_id) = sid.parse::<Uuid>() {
            // A layer is only safe to composite if its source is actually
            // producing video. overlay needs a first frame from every input;
            // a source that has a relay but no frames (e.g. a browser source
            // whose page never rendered) would otherwise stall the entire graph
            // and black out the whole program. Require a cached video sequence
            // header — proof the source has emitted decodable video — and skip
            // the layer otherwise.
            let has_relay = state.get_media_relay(&source_id).await.is_some();
            let has_video = state
                .sequence_headers
                .read()
                .await
                .get(&source_id)
                .map(|h| h.video.is_some())
                .unwrap_or(false);
            if has_relay && has_video {
                layers.push(CompLayer {
                    source_id,
                    x: x as i32,
                    y: y as i32,
                    w: (w as u32).max(2),
                    h: (h as u32).max(2),
                    opacity: op as f32,
                    fit: LayerFit::from_db(&fit),
                });
            } else {
                tracing::warn!(
                    "compositor: skipping layer {} (relay={}, video={}) — not producing video",
                    source_id, has_relay, has_video
                );
            }
        }
    }

    if layers.is_empty() {
        return Err("scene has no live layers".to_string());
    }

    let cfg = load_output_config(&state).await;
    let public_tx = state.get_or_create_media_relay(scene_id).await;

    // One TCP listener per layer — ffmpeg connects to each as an input.
    let mut listeners = Vec::new();
    let mut ports = Vec::new();
    for _ in &layers {
        let l = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("compositor listen: {}", e))?;
        ports.push(l.local_addr().map_err(|e| e.to_string())?.port());
        listeners.push(l);
    }

    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "warning".into(),
    ];
    for p in &ports {
        // Bound input probing. A near-static layer (e.g. a still image) has such
        // a low bitrate that ffmpeg's default probesize/analyzeduration is never
        // satisfied, so the input never finishes opening and the whole graph
        // stalls with no output. The primed sequence header gives codec params
        // immediately, so we can analyze briefly and proceed.
        args.push("-analyzeduration".into());
        args.push("500000".into());
        args.push("-probesize".into());
        args.push("500000".into());
        args.push("-f".into());
        args.push("flv".into());
        args.push("-i".into());
        args.push(format!("tcp://127.0.0.1:{}", p));
    }
    // Silent audio so the FLV is well-formed; program audio is muxed separately.
    args.push("-f".into());
    args.push("lavfi".into());
    args.push("-i".into());
    args.push("anullsrc=r=48000:cl=stereo".into());

    // Build the filter graph: black canvas, then scale + overlay each layer.
    let (filter, final_label) = build_filter_graph(&layers, cfg.width, cfg.height, cfg.fps);
    let audio_idx = layers.len();

    args.push("-filter_complex".into());
    args.push(filter);
    args.push("-map".into());
    args.push(format!("[{}]", final_label));
    args.push("-map".into());
    args.push(format!("{}:a", audio_idx));

    let bv = format!("{}k", cfg.video_bitrate_kbps);
    let bufsize = format!("{}k", cfg.video_bitrate_kbps * 2);
    let gop = format!("{}", cfg.fps * 2);
    let fps = format!("{}", cfg.fps);
    let ba = format!("{}k", cfg.audio_bitrate_kbps);
    for a in [
        "-c:v", "libx264", "-preset", "veryfast", "-tune", "zerolatency", "-b:v", &bv, "-maxrate",
        &bv, "-bufsize", &bufsize, "-g", &gop, "-r", &fps, "-pix_fmt", "yuv420p", "-c:a", "aac",
        "-b:a", &ba, "-ar", "48000", "-f", "flv", "-flvflags", "no_duration_filesize", "pipe:1",
    ] {
        args.push(a.to_string());
    }

    tracing::info!(
        "starting scene compositor for {} ({} layers, {}x{}@{}fps)",
        scene_id, layers.len(), cfg.width, cfg.height, cfg.fps
    );

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("failed to start compositor ffmpeg: {}", e))?;

    let stdout = child.stdout.take().ok_or("no stdout")?;
    if let Some(stderr) = child.stderr.take() {
        let sid = scene_id;
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
                    Ok(0) => break,
                    Ok(_) => tracing::debug!("compositor [{}]: {}", sid, line.trim()),
                    Err(_) => break,
                }
            }
        });
    }

    {
        let mut comps = state.scene_compositors.write().await;
        if let Some(mut old) = comps.insert(scene_id, child) {
            let _ = old.kill().await;
        }
    }

    // Feed each layer's source into its ffmpeg input.
    for (listener, layer) in listeners.into_iter().zip(layers.iter()) {
        let source_id = layer.source_id;
        let st = state.clone();
        tokio::spawn(async move {
            feed_layer(listener, source_id, st).await;
        });
    }

    // Read the composited output and publish it to the scene's relay.
    let st = state.clone();
    tokio::spawn(async move {
        read_composited_output(scene_id, stdout, public_tx, st).await;
    });

    Ok(())
}

/// Accept ffmpeg's connection for one layer and stream the source's FLV to it,
/// priming with the source's sequence headers so it decodes from the first byte.
async fn feed_layer(listener: TcpListener, source_id: Uuid, state: Arc<AppState>) {
    let Ok((mut sock, _)) = listener.accept().await else {
        return;
    };
    let _ = sock.set_nodelay(true);

    if sock.write_all(&crate::rtmp::flv::flv_header()).await.is_err() {
        return;
    }

    let Some(relay) = state.get_media_relay(&source_id).await else {
        return;
    };
    let mut rx = relay.subscribe();

    // Prime with cached sequence headers + last keyframe.
    {
        let headers = state.sequence_headers.read().await;
        if let Some(h) = headers.get(&source_id) {
            if let Some(v) = &h.video {
                let _ = sock.write_all(v).await;
            }
            if let Some(a) = &h.audio {
                let _ = sock.write_all(a).await;
            }
            if let Some(k) = &h.last_keyframe {
                let _ = sock.write_all(k).await;
            }
        }
    }

    loop {
        match rx.recv().await {
            Ok(data) => {
                if sock.write_all(&data).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => break,
        }
    }
}

async fn read_composited_output(
    scene_id: Uuid,
    mut stdout: tokio::process::ChildStdout,
    public_tx: broadcast::Sender<Bytes>,
    state: Arc<AppState>,
) {
    let mut header = [0u8; 13];
    if stdout.read_exact(&mut header).await.is_err() {
        tracing::warn!("compositor: failed to read FLV header for scene {}", scene_id);
        return;
    }

    let mut frame_count: u64 = 0;
    loop {
        let mut tag_header = [0u8; 11];
        if stdout.read_exact(&mut tag_header).await.is_err() {
            break;
        }
        let tag_type = tag_header[0];
        let data_size = ((tag_header[1] as u32) << 16)
            | ((tag_header[2] as u32) << 8)
            | (tag_header[3] as u32);

        let mut data = vec![0u8; data_size as usize];
        if stdout.read_exact(&mut data).await.is_err() {
            break;
        }
        let mut prev_size = [0u8; 4];
        if stdout.read_exact(&mut prev_size).await.is_err() {
            break;
        }

        let mut tag_buf = Vec::with_capacity(11 + data_size as usize + 4);
        tag_buf.extend_from_slice(&tag_header);
        tag_buf.extend_from_slice(&data);
        tag_buf.extend_from_slice(&prev_size);
        let tag = Bytes::from(tag_buf);

        if tag_type == 9 && !data.is_empty() {
            let avc_packet_type = if data.len() > 1 { data[1] } else { 255 };
            let frame_type = (data[0] >> 4) & 0x0F;
            if avc_packet_type == 0 {
                state.sequence_headers.write().await.entry(scene_id).or_default().video =
                    Some(tag.clone());
            } else if frame_type == 1 {
                let mut h = state.sequence_headers.write().await;
                if let Some(entry) = h.get_mut(&scene_id) {
                    entry.last_keyframe = Some(tag.clone());
                }
            }
        } else if tag_type == 8 && !data.is_empty() {
            let aac_packet_type = if data.len() > 1 { data[1] } else { 255 };
            if aac_packet_type == 0 {
                state.sequence_headers.write().await.entry(scene_id).or_default().audio =
                    Some(tag.clone());
            }
        }

        if frame_count == 0 {
            state
                .source_states
                .write()
                .await
                .insert(scene_id, muxshed_common::SourceState::Live);
            tracing::info!("compositor: scene {} is live", scene_id);
        }

        let _ = public_tx.send(tag);
        frame_count += 1;
    }

    tracing::info!("compositor: scene {} ended after {} frames", scene_id, frame_count);
    state.scene_compositors.write().await.remove(&scene_id);
}

pub async fn stop_scene_compositor(state: &AppState, scene_id: &Uuid) {
    if let Some(mut child) = state.scene_compositors.write().await.remove(scene_id) {
        let _ = child.kill().await;
    }
    state.remove_media_relay(scene_id).await;
    state.sequence_headers.write().await.remove(scene_id);
}

/// Stop every running scene compositor (e.g. when cutting to a single source).
pub async fn stop_all_compositors(state: &AppState) {
    let ids: Vec<Uuid> = state.scene_compositors.read().await.keys().copied().collect();
    for id in ids {
        stop_scene_compositor(state, &id).await;
    }
}

async fn load_output_config(state: &AppState) -> OutputConfig {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'output_config'")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|(json,)| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(x: i32, y: i32, w: u32, h: u32, opacity: f32) -> CompLayer {
        CompLayer { source_id: Uuid::nil(), x, y, w, h, opacity, fit: LayerFit::Fill }
    }

    fn layer_fit(x: i32, y: i32, w: u32, h: u32, fit: LayerFit) -> CompLayer {
        CompLayer { source_id: Uuid::nil(), x, y, w, h, opacity: 1.0, fit }
    }

    #[test]
    fn fill_stretches_to_box() {
        let (g, _) = build_filter_graph(&[layer_fit(0, 0, 640, 360, LayerFit::Fill)], 1920, 1080, 30);
        assert!(g.contains("[0:v]setpts=PTS-STARTPTS,scale=640:360[l0]"));
        assert!(g.contains("[base][l0]overlay=x=0:y=0:eof_action=pass[s0]"));
    }

    #[test]
    fn cover_scales_up_and_crops() {
        let (g, _) = build_filter_graph(&[layer_fit(100, 50, 640, 360, LayerFit::Cover)], 1920, 1080, 30);
        assert!(g.contains("scale=640:360:force_original_aspect_ratio=increase,crop=640:360"));
        assert!(g.contains("overlay=x=100:y=50:eof_action=pass"));
    }

    #[test]
    fn contain_fits_inside_and_centres() {
        let (g, _) = build_filter_graph(&[layer_fit(100, 50, 640, 360, LayerFit::Contain)], 1920, 1080, 30);
        assert!(g.contains("scale=640:360:force_original_aspect_ratio=decrease"));
        // centred within the box via runtime overlay-dimension expressions
        assert!(g.contains("overlay=x=100+(640-w)/2:y=50+(360-h)/2:eof_action=pass"));
    }

    #[test]
    fn single_opaque_layer() {
        let (g, out) = build_filter_graph(&[layer(0, 0, 1920, 1080, 1.0)], 1920, 1080, 30);
        assert_eq!(out, "s0");
        assert!(g.starts_with("color=c=black:s=1920x1080:r=30[base]"));
        assert!(g.contains("[0:v]setpts=PTS-STARTPTS,scale=1920:1080[l0]"));
        assert!(g.contains("[base][l0]overlay=x=0:y=0:eof_action=pass[s0]"));
        // opaque layer must NOT get an alpha multiply
        assert!(!g.contains("colorchannelmixer"));
    }

    #[test]
    fn pip_layer_is_positioned_and_scaled() {
        let (g, out) = build_filter_graph(
            &[layer(0, 0, 1920, 1080, 1.0), layer(1240, 700, 640, 360, 1.0)],
            1920, 1080, 30,
        );
        assert_eq!(out, "s1");
        assert!(g.contains("[1:v]setpts=PTS-STARTPTS,scale=640:360[l1]"));
        assert!(g.contains("[s0][l1]overlay=x=1240:y=700:eof_action=pass[s1]"));
    }

    #[test]
    fn semi_transparent_layer_gets_alpha() {
        let (g, _) = build_filter_graph(&[layer(40, 40, 480, 270, 0.5)], 1920, 1080, 30);
        assert!(g.contains("format=yuva420p,colorchannelmixer=aa=0.500"));
    }

    #[test]
    fn z_order_chains_overlays_in_input_order() {
        let (g, out) = build_filter_graph(
            &[layer(0, 0, 100, 100, 1.0), layer(0, 0, 100, 100, 1.0), layer(0, 0, 100, 100, 1.0)],
            1280, 720, 25,
        );
        assert_eq!(out, "s2");
        // each overlay chains onto the previous step
        assert!(g.contains("[base][l0]overlay"));
        assert!(g.contains("[s0][l1]overlay"));
        assert!(g.contains("[s1][l2]overlay"));
    }
}
