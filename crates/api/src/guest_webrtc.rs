// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Guest WHIP ingest — the invite-link flavor of WebRTC ingest.
//!
//! A named guest publishes camera/mic over WebRTC to a token-scoped endpoint.
//! The peer/RTP/ffmpeg/relay machinery lives in `crate::webrtc_ingest`; this
//! module only adds the guest-specific concerns (an ephemeral source row and the
//! guest record status). See `docs/guests-webrtc.md` for the live-path notes.

use std::sync::Arc;
use uuid::Uuid;

use crate::state::AppState;
use crate::webrtc_ingest::{self, IngestKind};

/// Accept a guest's WHIP offer and return the SDP answer. The ephemeral source
/// row must already exist (created by the caller); on error the caller tears it
/// down via `stop_guest_ingest`.
pub async fn start_guest_ingest(
    state: Arc<AppState>,
    source_id: Uuid,
    offer_sdp: String,
) -> Result<String, String> {
    webrtc_ingest::start_ingest(state, source_id, offer_sdp, IngestKind::Guest).await
}

/// Tear down a guest's ingest and drop its ephemeral source row.
pub async fn stop_guest_ingest(state: &AppState, source_id: &Uuid) {
    webrtc_ingest::stop_ingest(state, source_id, IngestKind::Guest).await
}
