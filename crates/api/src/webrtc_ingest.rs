// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Shared WebRTC (WHIP) ingest core.
//!
//! Terminates a WebRTC peer with webrtc-rs, forwards the publisher's RTP to
//! local UDP ports, and runs an ffmpeg subprocess that reads them via an SDP,
//! normalizes to the output canvas, and feeds FLV into the source's media relay
//! — so a WHIP publisher (OBS/hardware over H.264, or a browser over VP8)
//! becomes an ordinary, switchable Source. Both the persistent WHIP ingest
//! source (`routes::whip`) and the guest invite flow (`guest_webrtc`) call this.

/// Whether an ingest is a persistent WHIP source or an ephemeral guest.
/// On teardown a guest's source row is deleted; a persistent source is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestKind {
    Persistent,
    Guest,
}

/// The negotiated video codec, read from the publisher's track. Determines the
/// ffmpeg SDP and the fixed payload type we rewrite forwarded RTP to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    Vp8,
    H264,
}

/// Fixed payload types we rewrite forwarded RTP to, so the ffmpeg SDP is
/// deterministic regardless of the payload types the publisher negotiated.
const VP8_PT: u8 = 96;
const H264_PT: u8 = 97;
const OPUS_PT: u8 = 111;

impl VideoCodec {
    /// Map a track's negotiated mime type (e.g. "video/H264") to a codec.
    /// Anything that is not H.264 is treated as VP8.
    pub fn from_mime(mime: &str) -> VideoCodec {
        if mime.eq_ignore_ascii_case("video/H264") {
            VideoCodec::H264
        } else {
            VideoCodec::Vp8
        }
    }

    /// The fixed RTP payload type we rewrite this codec's packets to.
    pub fn payload_type(&self) -> u8 {
        match self {
            VideoCodec::Vp8 => VP8_PT,
            VideoCodec::H264 => H264_PT,
        }
    }

    fn rtpmap(&self) -> &'static str {
        match self {
            VideoCodec::Vp8 => "VP8/90000",
            VideoCodec::H264 => "H264/90000",
        }
    }
}

/// Build the ffmpeg input SDP for a given video codec and the two UDP ports the
/// publisher's RTP is forwarded to. Audio is always Opus on the fixed Opus PT.
pub fn build_ingest_sdp(video_port: u16, audio_port: u16, video: VideoCodec) -> String {
    let vpt = video.payload_type();
    let mut sdp = format!(
        "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=muxshed-ingest\r\nc=IN IP4 127.0.0.1\r\nt=0 0\r\n\
         m=video {vp} RTP/AVP {vpt}\r\na=rtpmap:{vpt} {rtpmap}\r\n",
        vp = video_port,
        vpt = vpt,
        rtpmap = video.rtpmap(),
    );
    // H.264 over RTP needs the packetization mode so ffmpeg reassembles FU-A/STAP-A.
    if video == VideoCodec::H264 {
        sdp.push_str(&format!("a=fmtp:{} packetization-mode=1\r\n", vpt));
    }
    sdp.push_str(&format!(
        "m=audio {ap} RTP/AVP {opt}\r\na=rtpmap:{opt} opus/48000/2\r\n",
        ap = audio_port,
        opt = OPUS_PT,
    ));
    sdp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_mime_maps_h264_and_defaults_vp8() {
        assert_eq!(VideoCodec::from_mime("video/H264"), VideoCodec::H264);
        assert_eq!(VideoCodec::from_mime("video/h264"), VideoCodec::H264);
        assert_eq!(VideoCodec::from_mime("video/VP8"), VideoCodec::Vp8);
        assert_eq!(VideoCodec::from_mime(""), VideoCodec::Vp8);
    }

    #[test]
    fn sdp_h264_has_rtpmap_and_packetization_mode() {
        let sdp = build_ingest_sdp(5000, 5002, VideoCodec::H264);
        assert!(sdp.contains("m=video 5000 RTP/AVP 97"));
        assert!(sdp.contains("a=rtpmap:97 H264/90000"));
        assert!(sdp.contains("a=fmtp:97 packetization-mode=1"));
        assert!(sdp.contains("m=audio 5002 RTP/AVP 111"));
        assert!(sdp.contains("a=rtpmap:111 opus/48000/2"));
    }

    #[test]
    fn sdp_vp8_has_no_packetization_mode() {
        let sdp = build_ingest_sdp(6000, 6002, VideoCodec::Vp8);
        assert!(sdp.contains("m=video 6000 RTP/AVP 96"));
        assert!(sdp.contains("a=rtpmap:96 VP8/90000"));
        assert!(!sdp.contains("packetization-mode"));
    }
}
