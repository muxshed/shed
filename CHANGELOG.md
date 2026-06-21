<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->

# Changelog

All notable changes to the Muxshed open-source core are documented here.
This project adheres to [Semantic Versioning](https://semver.org/).

## [1.0.0] — first public release

The first open-source release of Muxshed — a self-hosted live production studio.

### Added
- **Public Channel + watch page.** A self-hosted, publicly-watchable HLS rendition of
  the program output at an unlisted `/watch/<token>` link, with optional viewer password
  and custom branding (title, logo, accent colour). Configured from its own **Channel**
  section. Produced by an ffmpeg subprocess (`crates/api/src/channel_hls.rs`); starts
  automatically on go-live when enabled.
- **Toast notifications.** All operator-facing errors now surface as toasts
  (`svelte-sonner`) instead of inline text, via a central `notify` helper.
- **Reduce-effects toggle.** Disables scanlines/glow/pulse; also honours
  `prefers-reduced-motion`.

### Changed
- **Complete UI redesign — "Amber Rack".** A retro broadcast control-surface aesthetic:
  amber-on-near-black, monospace + 7-segment LED readouts, labelled rack panels, segmented
  meters, motion-safe CRT scanlines. Dark-only. See `DESIGN.md`.
- **Go Live no longer requires an external destination.** The program can always be
  broadcast to the public Channel watch page, so going live with zero RTMP/SRT
  destinations is valid.
- Fonts (JetBrains Mono, DSEG7) are now **bundled locally** — the UI works fully offline,
  no CDN dependency.

### Fixed
- Studio multi-source grid used a runtime Tailwind class (`grid-cols-${n}`) that the
  Tailwind v4 compiler never generated; replaced with an inline grid template.

### Notes
- **Media engine is ffmpeg**, not GStreamer. The GStreamer pipeline was never compiled
  into any build; the dead `set_channel`/`hlssink2` path has been removed. RTMP fan-out
  is forwarded by the Rust relay; HLS for the watch page is the only transcoded output.
