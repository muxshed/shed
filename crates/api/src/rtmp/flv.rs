// Licensed under the Business Source License 1.1 — see LICENSE.

use bytes::{BufMut, Bytes, BytesMut};

const FLV_TAG_AUDIO: u8 = 8;
const FLV_TAG_VIDEO: u8 = 9;

pub fn flv_header() -> Bytes {
    let mut buf = BytesMut::with_capacity(13);
    buf.put_slice(b"FLV");
    buf.put_u8(1); // version
    buf.put_u8(0x05); // audio + video flags
    buf.put_u32(9); // data offset
    buf.put_u32(0); // previous tag size (first)
    buf.freeze()
}

pub fn flv_audio_tag(data: &[u8], timestamp_ms: u32) -> Bytes {
    flv_tag(FLV_TAG_AUDIO, data, timestamp_ms)
}

pub fn flv_video_tag(data: &[u8], timestamp_ms: u32) -> Bytes {
    flv_tag(FLV_TAG_VIDEO, data, timestamp_ms)
}

/// Read the timestamp from an FLV tag (bytes 4-7).
pub fn read_tag_timestamp(tag: &[u8]) -> Option<u32> {
    if tag.len() < 8 {
        return None;
    }
    let lower = ((tag[4] as u32) << 16) | ((tag[5] as u32) << 8) | (tag[6] as u32);
    let upper = (tag[7] as u32) << 24;
    Some(upper | lower)
}

/// Rewrite the timestamp in an FLV tag, returning a new Bytes with the updated timestamp.
pub fn rewrite_tag_timestamp(tag: &[u8], new_ts: u32) -> Bytes {
    let mut buf = BytesMut::from(tag);
    if buf.len() >= 8 {
        buf[4] = (new_ts >> 16) as u8;
        buf[5] = (new_ts >> 8) as u8;
        buf[6] = new_ts as u8;
        buf[7] = (new_ts >> 24) as u8;
    }
    buf.freeze()
}

fn flv_tag(tag_type: u8, data: &[u8], timestamp_ms: u32) -> Bytes {
    let data_size = data.len() as u32;
    let total_tag_size = 11 + data_size;
    let mut buf = BytesMut::with_capacity((total_tag_size + 4) as usize);

    // Tag header (11 bytes)
    buf.put_u8(tag_type);
    // Data size (24-bit)
    buf.put_u8((data_size >> 16) as u8);
    buf.put_u8((data_size >> 8) as u8);
    buf.put_u8(data_size as u8);
    // Timestamp (24-bit lower + 8-bit upper)
    buf.put_u8((timestamp_ms >> 16) as u8);
    buf.put_u8((timestamp_ms >> 8) as u8);
    buf.put_u8(timestamp_ms as u8);
    buf.put_u8((timestamp_ms >> 24) as u8); // timestamp extended
    // Stream ID (always 0)
    buf.put_u8(0);
    buf.put_u8(0);
    buf.put_u8(0);

    // Tag data
    buf.put_slice(data);

    // Previous tag size
    buf.put_u32(total_tag_size);

    buf.freeze()
}
