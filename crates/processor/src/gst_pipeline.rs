// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![cfg(feature = "gstreamer")]

use async_trait::async_trait;
use gstreamer::prelude::*;
use muxshed_common::{DelayConfig, Destination, DestinationKind, MuxshedError, PipelineState, RecordingState, WsEvent};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::controller::PipelineController;

pub struct GstPipelineController {
    state: Arc<Mutex<PipelineState>>,
    pipeline: Arc<Mutex<Option<gstreamer::Pipeline>>>,
    recording: Arc<Mutex<RecordingState>>,
    ws_tx: broadcast::Sender<WsEvent>,
}

impl GstPipelineController {
    pub fn new(ws_tx: broadcast::Sender<WsEvent>) -> Result<Self, MuxshedError> {
        gstreamer::init().map_err(|e| MuxshedError::Pipeline(e.to_string()))?;
        Ok(Self {
            state: Arc::new(Mutex::new(PipelineState::Idle)),
            pipeline: Arc::new(Mutex::new(None)),
            recording: Arc::new(Mutex::new(RecordingState {
                recording: false,
                path: None,
                started_at: None,
            })),
            ws_tx,
        })
    }
}

#[async_trait]
impl PipelineController for GstPipelineController {
    async fn start(&self, destinations: Vec<Destination>) -> Result<(), MuxshedError> {
        let pipeline = gstreamer::Pipeline::with_name("muxshed-main");

        // RTMP ingest: flvdemux splits audio/video from incoming RTMP
        let rtmp_src = gstreamer::ElementFactory::make("rtmp2src")
            .name("rtmp_src")
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        let flvdemux = gstreamer::ElementFactory::make("flvdemux")
            .name("demux")
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        let videoconvert = gstreamer::ElementFactory::make("videoconvert")
            .name("vconvert")
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        let x264enc = gstreamer::ElementFactory::make("x264enc")
            .name("encoder")
            .property_from_str("tune", "zerolatency")
            .property("bitrate", 4500u32)
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        let flvmux = gstreamer::ElementFactory::make("flvmux")
            .name("mux")
            .property("streamable", true)
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        let audioconvert = gstreamer::ElementFactory::make("audioconvert")
            .name("aconvert")
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        let voaacenc = gstreamer::ElementFactory::make("voaacenc")
            .name("aencoder")
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        let tee = gstreamer::ElementFactory::make("tee")
            .name("output_tee")
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        pipeline
            .add_many([
                &rtmp_src,
                &flvdemux,
                &videoconvert,
                &x264enc,
                &flvmux,
                &audioconvert,
                &voaacenc,
                &tee,
            ])
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        gstreamer::Element::link_many([&rtmp_src, &flvdemux])
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        gstreamer::Element::link_many([&videoconvert, &x264enc, &flvmux, &tee])
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        gstreamer::Element::link_many([&audioconvert, &voaacenc])
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        // Add rtmpsink for each enabled destination
        for dest in &destinations {
            if let DestinationKind::Rtmp { url, stream_key } = &dest.kind {
                let queue = gstreamer::ElementFactory::make("queue")
                    .name(&format!("queue_{}", dest.id))
                    .build()
                    .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

                let sink = gstreamer::ElementFactory::make("rtmp2sink")
                    .name(&format!("sink_{}", dest.id))
                    .property("location", format!("{}/{}", url, stream_key))
                    .build()
                    .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

                pipeline
                    .add_many([&queue, &sink])
                    .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

                gstreamer::Element::link_many([&tee, &queue, &sink])
                    .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;
            }
        }

        // Set state and broadcast
        {
            let mut s = self.state.lock().await;
            *s = PipelineState::Starting;
        }
        let _ = self.ws_tx.send(WsEvent::PipelineState {
            state: PipelineState::Starting,
        });

        pipeline
            .set_state(gstreamer::State::Playing)
            .map_err(|e| MuxshedError::Pipeline(format!("failed to start pipeline: {:?}", e)))?;

        {
            let mut p = self.pipeline.lock().await;
            *p = Some(pipeline);
        }

        let live_state = PipelineState::Live {
            started_at: chrono::Utc::now(),
            active_scene: Uuid::nil(),
        };
        {
            let mut s = self.state.lock().await;
            *s = live_state.clone();
        }
        let _ = self.ws_tx.send(WsEvent::PipelineState { state: live_state });

        // Bus watch for errors
        let state = self.state.clone();
        let ws_tx = self.ws_tx.clone();
        let pipeline_ref = self.pipeline.clone();
        tokio::spawn(async move {
            let pipeline_lock = pipeline_ref.lock().await;
            if let Some(ref pipeline) = *pipeline_lock {
                let bus = pipeline.bus().expect("pipeline has no bus");
                drop(pipeline_lock);
                loop {
                    let Some(msg) = bus.timed_pop(gstreamer::ClockTime::from_mseconds(100)) else {
                        continue;
                    };
                    match msg.view() {
                        gstreamer::MessageView::Error(err) => {
                            let error_msg = err.error().to_string();
                            tracing::error!("pipeline error: {}", error_msg);
                            let error_state =
                                PipelineState::Error { message: error_msg.clone() };
                            {
                                let mut s = state.lock().await;
                                *s = error_state.clone();
                            }
                            let _ = ws_tx.send(WsEvent::PipelineState { state: error_state });
                            break;
                        }
                        gstreamer::MessageView::Eos(_) => {
                            tracing::info!("pipeline reached end of stream");
                            break;
                        }
                        _ => {}
                    }
                }
            }
        });

        Ok(())
    }

    async fn stop(&self) -> Result<(), MuxshedError> {
        {
            let mut s = self.state.lock().await;
            *s = PipelineState::Stopping;
        }
        let _ = self.ws_tx.send(WsEvent::PipelineState {
            state: PipelineState::Stopping,
        });

        {
            let mut p = self.pipeline.lock().await;
            if let Some(pipeline) = p.take() {
                pipeline
                    .set_state(gstreamer::State::Null)
                    .map_err(|e| {
                        MuxshedError::Pipeline(format!("failed to stop pipeline: {:?}", e))
                    })?;
            }
        }

        {
            let mut s = self.state.lock().await;
            *s = PipelineState::Idle;
        }
        let _ = self.ws_tx.send(WsEvent::PipelineState {
            state: PipelineState::Idle,
        });

        Ok(())
    }

    async fn state(&self) -> PipelineState {
        self.state.lock().await.clone()
    }

    async fn add_destination(&self, dest: &Destination) -> Result<(), MuxshedError> {
        let p = self.pipeline.lock().await;
        let Some(ref pipeline) = *p else {
            return Err(MuxshedError::Pipeline("pipeline not running".to_string()));
        };

        if let DestinationKind::Rtmp { url, stream_key } = &dest.kind {
            let tee = pipeline
                .by_name("output_tee")
                .ok_or_else(|| MuxshedError::Pipeline("tee not found".to_string()))?;

            let queue = gstreamer::ElementFactory::make("queue")
                .name(&format!("queue_{}", dest.id))
                .build()
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

            let sink = gstreamer::ElementFactory::make("rtmp2sink")
                .name(&format!("sink_{}", dest.id))
                .property("location", format!("{}/{}", url, stream_key))
                .build()
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

            pipeline
                .add_many([&queue, &sink])
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

            gstreamer::Element::link_many([&tee, &queue, &sink])
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

            queue
                .sync_state_with_parent()
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;
            sink.sync_state_with_parent()
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;
        }

        Ok(())
    }

    async fn remove_destination(&self, id: &Uuid) -> Result<(), MuxshedError> {
        let p = self.pipeline.lock().await;
        let Some(ref pipeline) = *p else {
            return Err(MuxshedError::Pipeline("pipeline not running".to_string()));
        };

        let queue_name = format!("queue_{}", id);
        let sink_name = format!("sink_{}", id);

        if let Some(queue) = pipeline.by_name(&queue_name) {
            queue
                .set_state(gstreamer::State::Null)
                .map_err(|e| MuxshedError::Pipeline(format!("{:?}", e)))?;
            pipeline
                .remove(&queue)
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;
        }

        if let Some(sink) = pipeline.by_name(&sink_name) {
            sink.set_state(gstreamer::State::Null)
                .map_err(|e| MuxshedError::Pipeline(format!("{:?}", e)))?;
            pipeline
                .remove(&sink)
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;
        }

        Ok(())
    }

    async fn activate_scene(&self, scene_id: &Uuid) -> Result<(), MuxshedError> {
        let mut state = self.state.lock().await;
        if let PipelineState::Live { started_at, .. } = state.clone() {
            *state = PipelineState::Live {
                started_at,
                active_scene: *scene_id,
            };
            let _ = self.ws_tx.send(WsEvent::PipelineState {
                state: state.clone(),
            });
        }
        // TODO: switch input-selector active pad based on scene layers
        Ok(())
    }

    async fn start_recording(&self, path: &PathBuf) -> Result<(), MuxshedError> {
        let p = self.pipeline.lock().await;
        let Some(ref pipeline) = *p else {
            return Err(MuxshedError::Pipeline("pipeline not running".to_string()));
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| MuxshedError::Pipeline(format!("cannot create recording dir: {}", e)))?;
        }

        let tee = pipeline
            .by_name("output_tee")
            .ok_or_else(|| MuxshedError::Pipeline("tee not found".to_string()))?;

        let queue = gstreamer::ElementFactory::make("queue")
            .name("rec_queue")
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        let filesink = gstreamer::ElementFactory::make("filesink")
            .name("rec_filesink")
            .property("location", path.display().to_string())
            .build()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        pipeline
            .add_many([&queue, &filesink])
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        gstreamer::Element::link_many([&tee, &queue, &filesink])
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        queue.sync_state_with_parent()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;
        filesink.sync_state_with_parent()
            .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;

        let mut rec = self.recording.lock().await;
        *rec = RecordingState {
            recording: true,
            path: Some(path.clone()),
            started_at: Some(chrono::Utc::now()),
        };

        let _ = self.ws_tx.send(WsEvent::RecordingState {
            recording: true,
            path: Some(path.display().to_string()),
        });

        Ok(())
    }

    async fn stop_recording(&self) -> Result<(), MuxshedError> {
        let p = self.pipeline.lock().await;
        let Some(ref pipeline) = *p else {
            return Err(MuxshedError::Pipeline("pipeline not running".to_string()));
        };

        if let Some(queue) = pipeline.by_name("rec_queue") {
            queue.set_state(gstreamer::State::Null)
                .map_err(|e| MuxshedError::Pipeline(format!("{:?}", e)))?;
            pipeline.remove(&queue)
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;
        }

        if let Some(sink) = pipeline.by_name("rec_filesink") {
            sink.set_state(gstreamer::State::Null)
                .map_err(|e| MuxshedError::Pipeline(format!("{:?}", e)))?;
            pipeline.remove(&sink)
                .map_err(|e| MuxshedError::Pipeline(e.to_string()))?;
        }

        let mut rec = self.recording.lock().await;
        *rec = RecordingState {
            recording: false,
            path: None,
            started_at: None,
        };

        let _ = self.ws_tx.send(WsEvent::RecordingState {
            recording: false,
            path: None,
        });

        Ok(())
    }

    async fn recording_state(&self) -> RecordingState {
        self.recording.lock().await.clone()
    }

    async fn set_delay(&self, config: &DelayConfig) -> Result<(), MuxshedError> {
        // TODO: configure GStreamer queue max-size-time for delay buffer
        tracing::info!("gst: set_delay enabled={} duration={}ms", config.enabled, config.duration_ms);
        Ok(())
    }

    async fn trigger_bleep(&self) -> Result<(), MuxshedError> {
        // TODO: set volume element to zero + mix 1kHz tone for 1 second
        let _ = self.ws_tx.send(WsEvent::BleepTriggered {
            at_ms: 0,
            source: "manual".to_string(),
        });
        Ok(())
    }

    async fn trigger_stinger_transition(
        &self,
        stinger_id: &Uuid,
        target_scene_id: &Uuid,
    ) -> Result<(), MuxshedError> {
        let _ = self.ws_tx.send(WsEvent::TransitionStarted {
            stinger_id: *stinger_id,
            target_scene_id: *target_scene_id,
        });
        // TODO: load stinger into overlay pad, wait for opaque point, switch source, wait for clear
        let ws_tx = self.ws_tx.clone();
        let state = self.state.clone();
        let scene_id = *target_scene_id;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            let mut s = state.lock().await;
            if let PipelineState::Live { started_at, .. } = s.clone() {
                *s = PipelineState::Live {
                    started_at,
                    active_scene: scene_id,
                };
                let _ = ws_tx.send(WsEvent::PipelineState { state: s.clone() });
            }
            let _ = ws_tx.send(WsEvent::TransitionComplete { scene_id });
        });
        Ok(())
    }
}
