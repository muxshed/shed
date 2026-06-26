# Guests — WebRTC (WHIP) ingest

Status: **foundation shipped; media core is the next focused build.**

Guests join via an unguessable invite link, allow camera/mic in the browser, and
**WHIP-publish** their stream to the instance. The published media becomes a normal
**Source** the switcher can cut to — so guests work with everything that already exists
(scenes, overlays, delay, recording, fan-out).

## What ships today

- **Invite lifecycle** (`crates/api/src/routes/guests.rs`): create / list / delete invites
  (operator, authed) + a `status` column (`invited` → `connected`) and a `source_id` link.
- **Public, token-scoped endpoints** (no auth):
  - `GET /api/v1/guest/{token}` — validate a link, return `{ name, status }`.
  - `POST /api/v1/guest/{token}/whip` — the WHIP endpoint (see below).
- **Guest join page** (`web/src/routes/guest/[token]`): validates the link, previews
  camera/mic, builds a real `RTCPeerConnection` offer and POSTs it to the WHIP endpoint.
  It degrades gracefully while ingest is being built (a `503` shows "coming soon").
- **Guests admin page** (`web/src/routes/(app)/guests`): invite, copy link, see status,
  delete.
- Integration tests cover invite/list/delete, public info (+404), and the WHIP contract.

The WHIP endpoint currently validates the token and returns `503 WEBRTC_NOT_READY`.

## The media core (to build)

Engine reality: Muxshed runs on **ffmpeg + the Rust RTMP relay** (no GStreamer). So:

1. **WHIP handshake** — accept the SDP offer at `POST /guest/{token}/whip`, create a
   `webrtc-rs` (`webrtc` crate) or `str0m` peer connection with a recvonly video + audio
   transceiver, set the remote description, create the answer, gather ICE, and return the
   SDP answer (`201 Created`, `Content-Type: application/sdp`, `Location` header per WHIP).
2. **Media → Source** — `on_track`: read the guest's RTP (VP8/VP9/H.264 + Opus),
   depacketize, and pipe into an **ffmpeg** subprocess that remuxes/transcodes to the
   instance's program format (FLV/H.264 + AAC), then register it as a Source feeding the
   existing `media_relays` / `program_tx` path (mirror `source_normalizer.rs`).
3. **Lifecycle** — on connect set `guests.status = 'connected'` and store `source_id`; on
   disconnect tear the source down and set `status = 'left'`. Emit `SourceState` WS events
   so the switcher and Companion module light up.
4. **TURN** — guests behind strict NAT need a TURN relay. Document an optional bundled
   `coturn`; advertise its ICE servers in the answer. LAN guests work with STUN only.

## Why this shape

- Self-contained: no SFU sidecar — `webrtc-rs`/`str0m` + ffmpeg keeps the "one binary +
  ffmpeg" story. The only optional extra service is TURN.
- Guests reuse the entire existing source/switcher/egress pipeline once they're a Source.
- WHIP is a single HTTP POST, so the browser side stays trivial and standards-based.

## Verification note

The media path can't be exercised in CI/headless — it needs a real browser, camera, and
(for non-LAN) TURN. Verify end-to-end against a running instance with two devices, and in
the Docker image where ffmpeg is present.
