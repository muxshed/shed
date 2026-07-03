# Headless Mode, Comprehensive API, Management Endpoints, and OpenAPI — Design

## Goal

Run Muxshed with no UI, for people who drive it from a CLI or the API. In headless mode the
API must be comprehensive (everything the UI can do is reachable over the API), efficient
(no wasted work serving a UI nobody loads), and expose a separate set of privileged
management endpoints for the managed-hosting portal. Ship an OpenAPI document with an
interactive docs page on every instance.

## Context

The system is already API-first. The Rust/Axum API (`crates/api`) is the whole engine; the
SvelteKit UI is a static bundle the API serves from `MUXSHED_WEB_DIR`. Headless is therefore
additive, not a re-architecture: one binary, one API, the UI optional on top. This design
does not split the API into a separate service.

## Decisions (agreed with the owner)

- Headless is a flag on the same binary and image: `MUXSHED_HEADLESS=true`.
- The first API key is provided by the environment at setup time.
- Management uses a dedicated token, separate from tenant keys. Each machine is single tenant.
- Deliver the API plus OpenAPI. No first-party CLI in this work.
- The `/admin` surface covers health, stats, connectivity, restart, and config.
- The OpenAPI docs page is public on every instance.

## Design

### 1. Headless flag

- Add `headless: bool` to `MuxshedConfig`, read from `MUXSHED_HEADLESS`.
- When true, the router does not mount the static-file and SPA-fallback layer. All
  `/api/v1/*` routes are unchanged. Unmatched non-API routes return a JSON 404, and `GET /`
  returns a small JSON notice with the version and a link to `/api/v1/docs`.
- `MUXSHED_WEB_DIR` is ignored in headless.

### 2. First key from the environment

- Add `bootstrap_api_key: Option<String>` from `MUXSHED_API_KEY`.
- On startup, if it is set, upsert an API key row with that exact value (SHA-256 hashed,
  `Admin` scope, name "bootstrap") and mark setup complete. It is idempotent: re-seeding the
  same value on restart is a no-op.
- This bypasses the web setup wizard. The web admin user is not created in headless. If the
  UI is later enabled on the instance, the normal setup flow still works.
- `GET /api/v1/setup/status` reports complete once a key exists.
- The portal generates a strong key and sets it at provision time, so it already knows the
  credential. The key is stored hashed and never logged.

### 3. Management endpoints

- Add `management_token: Option<String>` from `MUXSHED_MANAGEMENT_TOKEN`.
- A new `management_auth` middleware checks the `X-Management-Token` header with a
  constant-time comparison against the configured token. If no token is configured, the
  `/admin` group is not mounted at all, so self-hosters without a portal have no admin
  surface to attack.
- Route group `/api/v1/admin`, gated by `management_auth`, entirely separate from the tenant
  `auth` middleware so tenant API keys cannot reach it:
  - `GET /admin/health` — version, uptime, pipeline state, headless flag.
  - `GET /admin/stats` — active streams, source and destination states, bytes and bitrate in
    and out, CPU and memory.
  - `GET /admin/connectivity` — RTMP and SRT listeners bound, destination reachability from
    the last egress state, whether a source is live.
  - `POST /admin/restart` — graceful shutdown for the supervisor to restart, or an in-process
    reload where possible.
  - `GET /admin/config` and `PUT /admin/config` — read and push the mutable subset of
    instance config.
- The `/admin` group is excluded from the public OpenAPI (see section 4).

### 4. OpenAPI

- Add `utoipa` and `utoipa-scalar` to `crates/api`.
- Derive `ToSchema` on the shared types in `crates/common`, and annotate handlers with
  `#[utoipa::path(...)]`.
- Build the `OpenApi` document and serve `GET /api/v1/openapi.json` plus an interactive docs
  page at `GET /api/v1/docs` rendered with Scalar. Public on every instance.
- The public spec documents tenant endpoints only. The `/admin` group is described in a
  separate internal document at `GET /api/v1/admin/openapi.json`, gated by the management
  token, for the portal.
- Security scheme on the public spec: `X-API-Key` as an apiKey header.

### 5. Efficiency

- Headless skips static serving.
- Add `docker/Dockerfile.headless`: a single Rust build stage plus the ffmpeg runtime, with
  no Node build stage. Smaller image and faster CI. The default UI image and compose files
  are unchanged.

## Non-goals

- No separate headless service or second API. One API, UI optional.
- No first-party CLI in this work. The OpenAPI spec makes one straightforward later.
- No change to the media pipeline.

## Testing

- Headless boot: with `MUXSHED_HEADLESS=true` and `MUXSHED_API_KEY` set, the key
  authenticates, `/` returns the JSON notice, a static asset path returns 404, and a core API
  flow (create a source, list sources) works.
- Bootstrap: the env key seeds a working credential, setup-status reports complete, and a
  wrong key is rejected. Re-seeding the same key on restart does not duplicate rows.
- Management: `/admin/*` requires the management token, a tenant API key is rejected, each
  endpoint returns the expected shape, and the `/admin` group is absent when no management
  token is configured.
- OpenAPI: `/openapi.json` is valid, `/docs` renders, every annotated route appears, and the
  `/admin` group is excluded from the public spec.

## Phasing (each phase is shippable on its own)

1. Headless flag, env-key bootstrap, slim headless image.
2. Management token, `management_auth`, and the `/admin` endpoints for the portal.
3. utoipa OpenAPI plus the Scalar docs page over the public API, and an audit of every UI
   action to confirm it has an endpoint, filling any gaps found.
