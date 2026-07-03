# VOD Scheduling — Phase 2 Implementation Plan (Playlists & 24-7 channel)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use `- [ ]`.

**Goal:** A schedule can play a **playlist of VODs** (not just one), seamlessly, and **loop** it into an always-on 24-7 channel.

**Architecture:** Pure-VOD playlists play through a single ffmpeg **concat** process (seamless between items), scaled/padded to the output canvas so mixed-resolution VODs concatenate cleanly. `end_behavior: loop` uses `-stream_loop -1`. Reuses the existing `feed_relay` FLV→relay pump, `program_intent`, egress + egress-restart. The data model (`schedule_items`) already supports multiple ordered items — this is additive.

**Tech Stack:** Rust (Tokio, sqlx, ffmpeg concat demuxer), SvelteKit (Svelte 5, Tailwind).

**Base:** branch `feat/vod-scheduling-p2` off `main` (Phase 1 merged). Conventions per `system/CLAUDE.md`.

---

## Task 1: Concat playlist playback in the media player

**File:** `crates/api/src/media_player.rs` (add a function; reuse the existing `feed_relay`).

- [ ] **Step 1: Add `start_concat_playback`.** Append to `media_player.rs`:

```rust
/// Play a playlist of video files as one seamless stream into the source's
/// relay. Files are concatenated via ffmpeg's concat demuxer and scaled/padded
/// to the output canvas so mixed-resolution VODs join cleanly. `loop_forever`
/// repeats the whole playlist (a 24-7 channel).
pub async fn start_concat_playback(
    state: Arc<AppState>,
    source_id: Uuid,
    files: &[std::path::PathBuf],
    loop_forever: bool,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(), String> {
    if files.is_empty() {
        return Err("empty playlist".into());
    }
    let relay_tx = state.get_or_create_media_relay(source_id).await;

    // Write a concat list file. Single quotes in paths are escaped per the
    // concat demuxer's rules ( ' -> '\'' ).
    let data_dir = state.config.read().await.data_dir.clone();
    let _ = tokio::fs::create_dir_all(&data_dir).await;
    let list_path = data_dir.join(format!("playlist_{}.txt", source_id));
    let mut list = String::from("ffconcat version 1.0\n");
    for f in files {
        let p = f.to_string_lossy().replace('\'', "'\\''");
        list.push_str(&format!("file '{}'\n", p));
    }
    tokio::fs::write(&list_path, list).await.map_err(|e| format!("write playlist: {}", e))?;

    let vf = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,setsar=1",
        width, height, width, height
    );
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "warning".into()];
    if loop_forever {
        args.extend(["-stream_loop".into(), "-1".into()]);
    }
    args.extend([
        "-re".into(), "-f".into(), "concat".into(), "-safe".into(), "0".into(),
        "-i".into(), list_path.to_string_lossy().into_owned(),
        "-vf".into(), vf,
        "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(),
        "-tune".into(), "zerolatency".into(), "-pix_fmt".into(), "yuv420p".into(),
        "-g".into(), format!("{}", fps * 2), "-r".into(), format!("{}", fps), "-b:v".into(), "3000k".into(),
        "-c:a".into(), "aac".into(), "-b:a".into(), "128k".into(), "-ar".into(), "48000".into(),
        "-f".into(), "flv".into(), "pipe:1".into(),
    ]);

    tracing::info!("starting concat playback for {} ({} items, loop={})", source_id, files.len(), loop_forever);
    let mut child = Command::new("ffmpeg")
        .args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .kill_on_drop(true).spawn().map_err(|e| format!("failed to start ffmpeg: {}", e))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    if let Some(stderr) = child.stderr.take() {
        let sid = source_id;
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
                    Ok(0) => break,
                    Ok(_) => tracing::debug!("ffmpeg concat [{}]: {}", sid, line.trim()),
                    Err(_) => break,
                }
            }
        });
    }
    {
        let mut players = state.media_players.write().await;
        if let Some(mut old) = players.remove(&source_id) { let _ = old.kill().await; }
        players.insert(source_id, child);
    }
    let sc = state.clone();
    tokio::spawn(async move { feed_relay(source_id, stdout, relay_tx, sc).await; });
    Ok(())
}
```

- [ ] **Step 2: Build.** `cargo build -p muxshed-api` → Finished. (Confirm `feed_relay` is reachable from the new fn — it's a private fn in the same module, so a direct call works. Confirm `state.config.read().await.data_dir` is the right accessor by checking how other code reads `config`.)

- [ ] **Step 3: Commit.**
```bash
git add crates/api/src/media_player.rs
git commit -m "feat(schedules): concat playlist playback (seamless, loopable)"
```

---

## Task 2: Playout controller — play the whole playlist

**File:** `crates/api/src/playout.rs`

- [ ] **Step 1: Replace single-VOD loading with the full ordered playlist.** Replace the `load_first_vod` function (and its call) so the controller loads ALL `vod` items in order plus the total duration. Add:

```rust
struct Playlist { files: Vec<std::path::PathBuf>, total_ms: u64 }

async fn load_playlist(state: &AppState, schedule_id: Uuid) -> Result<Playlist, String> {
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT ref_id FROM schedule_items WHERE schedule_id = ? AND item_kind = 'vod' ORDER BY position ASC",
    ).bind(schedule_id.to_string()).fetch_all(&state.db).await.map_err(|e| e.to_string())?;
    if rows.is_empty() { return Err("schedule has no VOD items".into()); }
    let mut files = Vec::new();
    let mut total_ms = 0u64;
    for (ref_id,) in rows {
        let a = sqlx::query_as::<_, (String, i64)>("SELECT file_path, duration_ms FROM assets WHERE id = ?")
            .bind(&ref_id).fetch_optional(&state.db).await.map_err(|e| e.to_string())?;
        if let Some((path, dur)) = a { files.push(std::path::PathBuf::from(path)); total_ms += dur.max(0) as u64; }
    }
    if files.is_empty() { return Err("no playable VOD assets".into()); }
    Ok(Playlist { files, total_ms })
}
```

- [ ] **Step 2: Use it in `run_broadcast`.** In `run_broadcast`, replace the single-VOD block (the `load_first_vod` call, the `start_media_playback` call, and the `duration_ms`-based wait) with the playlist version:
  - Load `let playlist = load_playlist(state, schedule_id).await?;` near the top (replacing `let vod = load_first_vod(...)`). Keep the existing `end_behavior`, `standby`, `destinations`, egress start, and standby routing exactly as they are.
  - Where it currently starts the VOD, use the concat player and the output canvas from `cfg`:
    ```rust
    let vod_source = Uuid::new_v4();
    let loop_forever = matches!(end_behavior, EndBehavior::Loop);
    crate::media_player::start_concat_playback(
        state.clone(), vod_source, &playlist.files, loop_forever, cfg.width, cfg.height, cfg.fps,
    ).await.map_err(|e| format!("playlist playback: {}", e))?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let _ = state.program_intent.send(Some(vod_source));
    crate::egress_restart_for(state, vod_source).await;
    ```
  - The end wait: keep the existing structure but use the playlist total. When NOT looping:
    ```rust
    let dur = std::time::Duration::from_millis(playlist.total_ms.max(1000));
    tokio::select! {
        _ = tokio::time::sleep(dur) => {}
        _ = wait_until_deactivated(state, schedule_id) => {
            crate::media_player::stop_media_playback(state, &vod_source).await;
            return Ok(());
        }
    }
    crate::media_player::stop_media_playback(state, &vod_source).await;
    // ... existing end_behavior match (Standby / Stop) unchanged, but where it referenced `vod_source` keep it ...
    ```
  - Remove the now-unused `VodItem`/`load_first_vod`/`load_standby_asset`'s VodItem if `load_standby_asset` still needs a file path: keep `load_standby_asset` (it returns a path for the standby); if it used the `VodItem` struct, change it to return `Option<std::path::PathBuf>` and adjust `ensure_standby` accordingly. Keep behavior identical.

- [ ] **Step 3: Build + clippy.** `cargo build -p muxshed-api` (Finished) and `cargo clippy -p muxshed-api --all-targets -- -D warnings` (clean). Fix leftover unused items (delete `load_first_vod` / unused `VodItem` fields). Ensure `cargo test -p muxshed-api` still passes (the Phase-1 CRUD test is unaffected).

- [ ] **Step 4: Commit.**
```bash
git add crates/api/src/playout.rs
git commit -m "feat(schedules): playout plays the whole VOD playlist (loop = channel)"
```

---

## Task 3: Frontend playlist editor

**File:** `web/src/routes/(app)/schedules/+page.svelte`

- [ ] **Step 1: Replace the single VOD `<select>` with a playlist editor.** In the `<script>`, replace `let vodId = $state('')` with a list plus an add-selector:
```ts
	let items = $state<string[]>([]);   // ordered VOD asset ids
	let addVod = $state('');
```
In `newSchedule()`, set `items = []; addVod = assets[0]?.id ?? '';`. In `payload()`, change `items` to map the list: `items: items.map((ref_id) => ({ kind: 'vod', ref_id }))`. In `save()`, validate `if (items.length === 0) { notify.error('Add at least one VOD'); return; }` (replacing the `!vodId` check). When editing an existing schedule (if edit-load is added later), it's fine to leave `items` empty for now — Phase 1 create/edit already round-trips `items`.
  Add helpers:
```ts
	function addItem() { if (addVod) items = [...items, addVod]; }
	function removeItem(i: number) { items = items.filter((_, j) => j !== i); }
	function move(i: number, d: -1 | 1) {
		const j = i + d; if (j < 0 || j >= items.length) return;
		const c = [...items]; [c[i], c[j]] = [c[j], c[i]]; items = c;
	}
	const assetName = (id: string) => assets.find((a) => a.id === id)?.name ?? id;
```

- [ ] **Step 2: Replace the VOD select markup** (the `<label for="s-vod">VOD</label>` + its `<select>`) with a playlist block:
```svelte
				<span class="field-label">Playlist</span>
				<div class="mb-2 flex flex-col gap-1">
					{#each items as id, i (id + i)}
						<div class="row items-center justify-between text-xs">
							<span class="text-amber">{i + 1}. {assetName(id)}</span>
							<div class="flex gap-1">
								<button type="button" class="btn btn--ghost" onclick={() => move(i, -1)} aria-label="Move up">▲</button>
								<button type="button" class="btn btn--ghost" onclick={() => move(i, 1)} aria-label="Move down">▼</button>
								<button type="button" class="btn btn--danger" onclick={() => removeItem(i)} aria-label="Remove">✕</button>
							</div>
						</div>
					{/each}
					{#if items.length === 0}<p class="text-[11px] text-amber-muted">No VODs yet.</p>{/if}
				</div>
				<div class="mb-3 flex gap-2">
					<select bind:value={addVod} class="select flex-1">
						{#each assets as a}<option value={a.id}>{a.name}</option>{/each}
					</select>
					<button type="button" class="btn" onclick={addItem}>+ Add</button>
				</div>
```
Also update the "Loop" option help so it's clear loop = a 24-7 channel: leave the `end_behavior` select as is (stop / loop / standby).

- [ ] **Step 3: Verify.** `cd web && npm run check` (0 errors) and `npm run build` (succeeds). Fix any type issues.

- [ ] **Step 4: Commit.**
```bash
git add "web/src/routes/(app)/schedules/+page.svelte"
git commit -m "feat(schedules): playlist editor (multiple VODs, reorder)"
```

---

## Task 4: End-to-end verification (controller-run, real ffmpeg)

- [ ] **Step 1:** Create a script (mirroring `scratchpad/run_sched_e2e.sh` + the `nms/sink.js` persistent RTMP sink) that: starts the API on isolated ports; uploads **two distinct** short VODs (e.g. `testsrc2` and `smptebars`, ~4s each, visibly different); creates an RTMP destination → the sink; sets tz=UTC; creates a one-off schedule firing ~8s out whose **two** VOD items are both assets, `end_behavior: stop`. Wait through fire + playout.

- [ ] **Step 2:** Assert: the scheduler fires; the sink records the broadcast; extract a frame from **early** and a frame from **late** in the recorded session and confirm they show **different** VODs (item 1 then item 2 aired in sequence — the playlist advanced seamlessly). Then run a second schedule with `end_behavior: loop` and confirm the stream keeps producing past the summed duration (channel loops). Capture frames as proof.

- [ ] **Step 3:** Browser smoke test: open the Schedules editor, add two VODs to the playlist, reorder, save → confirm the DB has 2 `schedule_items` in the chosen order.

- [ ] **Step 4:** Commit any fixes; run full `cargo test --workspace` + `clippy --all-targets` + `svelte-check`; open a PR "VOD scheduling — Phase 2 (playlists & channel)".

---

## Self-review

- **Spec coverage:** multiple ordered VOD items via concat (T1/T2) · `loop` = 24-7 channel (T1/T2) · playlist editor (T3) · seamless-transition claim verified by the sequence e2e (T4). Scene/source items + run-history UI + cron builder remain for Phase 3.
- **Type consistency:** `start_concat_playback(state, source_id, &[PathBuf], bool, u32,u32,u32)` used identically in T1 and T2; `Playlist { files, total_ms }` local to playout.
- **Reuse:** T1 reuses `feed_relay` verbatim; T2 keeps the Phase-1 standby/egress/end structure, swapping only the source from single-VOD to concat.
- **Executor notes:** confirm `state.config.read().await.data_dir`; delete `load_first_vod` + unused `VodItem` bits after T2; concat demuxer needs `-safe 0` for absolute paths; mixed-resolution handled by the `scale+pad` vf (reinit on resolution change).
