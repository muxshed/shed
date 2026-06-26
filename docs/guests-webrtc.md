# Guests — WebRTC (WHIP) ingest

Status: **implemented** (`webrtc` crate 0.11). The live media path is verifiable only
against a real browser — see the verification note.

Guests join via an unguessable invite link, allow camera/mic in the browser, and
**WHIP-publish** their stream to the instance. The published media becomes a normal
**Source** the switcher can cut to — so guests work with everything that already exists
(scenes, overlays, delay, recording, fan-out).

## What ships

- **Invite lifecycle** (`crates/api/src/routes/guests.rs`): create / list / delete invites
  (operator, authed) + a `status` column (`invited` → `connecting` → `connected` → `left`)
  and a `source_id` link.
- **Public, token-scoped endpoints** (no auth):
  - `GET /api/v1/guest/{token}` — validate a link, return `{ name, status }`.
  - `POST /api/v1/guest/{token}/whip` — WHIP: takes the SDP offer, returns the SDP answer
    (`201 Created`, `Content-Type: application/sdp`, `Location` header). Malformed offers
    are rejected `400` and the ephemeral source is rolled back.
- **The bridge** (`crates/api/src/guest_webrtc.rs`):
  1. `webrtc-rs` peer (MediaEngine pinned to **VP8 + Opus**), `set_remote_description` →
     `create_answer` → ICE gather → answer.
  2. `on_track` reads RTP, rewrites the payload type to fixed values (VP8 96 / Opus 111),
     and forwards to local UDP ports.
  3. An **ffmpeg** subprocess reads those ports via a generated SDP
     (`-protocol_whitelist file,crypto,data,rtp,udp`), normalizes to the output canvas
     (libx264 + AAC), and feeds FLV into the source's `media_relays` channel — exactly the
     path `source_normalizer.rs` / `srt.rs` use. Source goes **Live** on the first frame.
  4. On peer disconnect (or source delete) `stop_guest_ingest` kills ffmpeg, closes the
     peer, drops the ephemeral source row, clears the relay/headers, and resets the guest
     to `left`. `SourceState` WS events fire throughout.
- **Guest join page** (`web/src/routes/guest/[token]`): validates the link, previews
  camera/mic, builds the WHIP offer, applies the answer.
- **Guests admin page** (`web/src/routes/(app)/guests`): invite, copy link, see status,
  delete.
- Integration tests cover invite/list/delete, public info (+404), and the WHIP contract
  (unknown token → 404, malformed offer → 400 with rollback).

## Connectivity (STUN / TURN)

ICE servers are configurable and stored in `settings` under `webrtc_config`
(`crates/api/src/routes/webrtc_config.rs`). The **same list is used by the server peer and
handed to the guest browser** via `GET /guest/{token}` (`ice_servers`), so the two always
agree. Default: a single public STUN server.

- **API**: `GET`/`PUT /api/v1/webrtc/config` (operator).
- **UI**: Guests → Connectivity (TURN) — set a TURN URL + username/credential, or leave
  blank for STUN-only.
- **TURN relay**: guests behind strict/symmetric NAT need TURN. An optional `coturn` service
  ships in `docker/docker-compose.yml` under the `turn` profile:

  ```sh
  docker compose --profile turn up      # starts muxshed + coturn (host networking, :3478)
  ```

  Then point the Connectivity form at `turn:<host>:3478` with the configured
  username/credential (change the `guest:changeme` default in the compose file). LAN guests
  work without TURN.

## Not yet done

- **Codec breadth** — pinned to VP8 + Opus for a deterministic ffmpeg SDP. H.264 guests
  would need dynamic SDP/PT handling.
- **TURN REST credentials** — only static long-term credentials are supported; coturn's
  time-limited (HMAC) credential REST API is not yet wired.
- ffmpeg in the runtime image must have the **VP8 decoder** (libvpx) and **libx264** — both
  are present in the standard Ubuntu ffmpeg the Docker image installs.

## Why this shape

- Self-contained: no SFU sidecar — `webrtc-rs`/`str0m` + ffmpeg keeps the "one binary +
  ffmpeg" story. The only optional extra service is TURN.
- Guests reuse the entire existing source/switcher/egress pipeline once they're a Source.
- WHIP is a single HTTP POST, so the browser side stays trivial and standards-based.

## Verification note

The media path can't be exercised in CI/headless — it needs a real browser, camera, and
(for non-LAN) TURN. Verify end-to-end against a running instance with two devices, and in
the Docker image where ffmpeg is present.
