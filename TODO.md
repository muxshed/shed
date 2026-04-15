# TODO

## GStreamer Pipeline (crates/processor)

- [ ] Switch input-selector active pad based on scene layers in activate_scene
- [ ] Configure GStreamer queue max-size-time for delay buffer in set_delay
- [ ] Set volume element to zero + mix 1kHz tone for 1 second in trigger_bleep
- [ ] Load stinger into overlay pad, wait for opaque point, switch source, wait for clear in trigger_stinger_transition

## Ingest

- [ ] WHIP (WebRTC HTTP Ingest Protocol) support -- requires webrtc crate for SDP signaling, pipe RTP to FFmpeg for normalization

## Features

- [ ] Role-based access enforcement in middleware (admin/write/read privilege checks on endpoints)
- [ ] SRT stream ID multiplexing (single port, multiple sources)
- [ ] Whisper.cpp auto-bleep integration
- [ ] Stream Deck plugin
- [ ] Bitfocus Companion module
