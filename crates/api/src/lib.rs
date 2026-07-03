// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

pub mod auth;
pub mod channel_hls;
pub mod openapi;
pub mod egress;
pub mod failover;
pub mod guest_webrtc;
pub mod media_player;
pub mod media_probe;
pub mod playout;
pub mod source_normalizer;
pub mod srt;
pub mod error;
pub mod program;
pub mod routes;
pub mod rtmp;
pub mod browser_source;
pub mod scene_compositor;
pub mod schedule_time;
pub mod scheduler;
pub mod state;

/// Restart the egress with a fresh encoder primed for `source`, when live.
/// Shared by the failover supervisor and the playout controller.
pub async fn egress_restart_for(state: &std::sync::Arc<crate::state::AppState>, source: uuid::Uuid) {
    if state.egress.is_running().await {
        let seq = state.sequence_headers.read().await.get(&source).cloned();
        state.egress.restart(seq).await;
    }
}
