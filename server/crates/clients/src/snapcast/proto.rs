//! Snapcast's binary protocol (snapcast's doc/binary_protocol.md; snapserver's own source read for the details,
//! 2026-10-16): every message is a 26-byte header, little-endian, then its payload. Times are seconds and microseconds of
//! the sender's steady clock; nothing compares them with another clock but through the time exchange.

use tokio::io::{AsyncRead, AsyncReadExt};

/// The header's length.
pub const BASE: usize = 26;
/// A payload longer than this is no message of ours (a wrong peer): the connection closes.
const MAX_PAYLOAD: u32 = 4 << 20;

pub const CODEC_HEADER: u16 = 1;
pub const WIRE_CHUNK: u16 = 2;
pub const SERVER_SETTINGS: u16 = 3;
pub const TIME: u16 = 4;
pub const HELLO: u16 = 5;
/// Cratarr's own, never sent by a snapclient: a group's leader says it sends the group's audio (JSON `{"player"}`).
pub const SOURCE: u16 = 100;
/// Cratarr's own: the leader's audio broke off (a seek, a jump, a pause); what was sent and not yet played goes.
pub const BREAK: u16 = 101;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Header {
    pub kind: u16,
    pub id: u16,
    pub refers_to: u16,
    /// When the sender sent it, by its clock (µs).
    pub sent_us: i64,
    /// When it came, by the receiver's clock (µs): filled in on reading.
    pub received_us: i64,
    pub size: u32,
}

/// Microseconds as snapcast's `tv`: whole seconds (rounded down) and the microseconds after them.
fn tv(us: i64) -> (i32, i32) {
    (us.div_euclid(1_000_000) as i32, us.rem_euclid(1_000_000) as i32)
}

fn us(sec: i32, usec: i32) -> i64 {
    i64::from(sec) * 1_000_000 + i64::from(usec)
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    u32_at(b, at) as i32
}

impl Header {
    pub fn parse(b: &[u8; BASE]) -> Self {
        Self {
            kind: u16_at(b, 0),
            id: u16_at(b, 2),
            refers_to: u16_at(b, 4),
            sent_us: us(i32_at(b, 6), i32_at(b, 10)),
            received_us: us(i32_at(b, 14), i32_at(b, 18)),
            size: u32_at(b, 22),
        }
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.kind.to_le_bytes());
        out.extend_from_slice(&self.id.to_le_bytes());
        out.extend_from_slice(&self.refers_to.to_le_bytes());
        for t in [self.sent_us, self.received_us] {
            let (s, u) = tv(t);
            out.extend_from_slice(&s.to_le_bytes());
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&self.size.to_le_bytes());
    }
}

/// A whole message, sent `sent_us` (stamped just before it is written).
pub fn message(kind: u16, id: u16, refers_to: u16, sent_us: i64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(BASE + payload.len());
    Header { kind, id, refers_to, sent_us, received_us: 0, size: payload.len() as u32 }.write(&mut out);
    out.extend_from_slice(payload);
    out
}

/// One message from `r`, its arrival stamped by `now` as soon as its header is in.
pub async fn read(r: &mut (impl AsyncRead + Unpin), now: impl Fn() -> i64) -> std::io::Result<(Header, Vec<u8>)> {
    let mut head = [0u8; BASE];
    r.read_exact(&mut head).await?;
    let mut h = Header::parse(&head);
    h.received_us = now();
    if h.size > MAX_PAYLOAD {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("a message of {} bytes", h.size)));
    }
    let mut payload = vec![0u8; h.size as usize];
    r.read_exact(&mut payload).await?;
    Ok((h, payload))
}

/// A string as snapcast writes one: its length, then its bytes.
fn put_str(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s);
}

/// The string at the start of `b`, and what follows it.
fn take_str(b: &[u8]) -> Option<(&[u8], &[u8])> {
    let len = u32_at(b.get(..4)?, 0) as usize;
    let s = b.get(4..4 + len)?;
    Some((s, &b[4 + len..]))
}

/// A JSON message's payload (Hello, Server Settings).
pub fn json(v: &serde_json::Value) -> Vec<u8> {
    let mut out = Vec::new();
    put_str(&mut out, v.to_string().as_bytes());
    out
}

pub fn read_json(payload: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice(take_str(payload)?.0).ok()
}

/// What a client is told of how to play: its buffer (from a chunk's stamp to its sound), its own extra delay, and its
/// volume.
pub fn server_settings(buffer_ms: u32, latency_ms: i32, volume: u8, muted: bool) -> Vec<u8> {
    json(&serde_json::json!({ "bufferMs": buffer_ms, "latency": latency_ms, "volume": volume.min(100), "muted": muted }))
}

/// A Time message's payload: a latency.
pub fn time(latency_us: i64) -> Vec<u8> {
    let (s, u) = tv(latency_us);
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&s.to_le_bytes());
    out.extend_from_slice(&u.to_le_bytes());
    out
}

/// Plain PCM's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub rate: u32,
    pub bits: u16,
    pub channels: u16,
}

impl Format {
    /// Bytes per frame (one sample of every channel).
    pub fn frame_bytes(&self) -> usize {
        usize::from(self.channels) * usize::from(self.bits.div_ceil(8))
    }

    /// How long `bytes` of it play (µs).
    pub fn duration_us(&self, bytes: usize) -> i64 {
        let frames = (bytes / self.frame_bytes().max(1)) as i64;
        frames * 1_000_000 / i64::from(self.rate.max(1))
    }
}

/// The codec header of plain PCM: the codec's name and a RIFF WAVE header, as snapserver's PCM encoder writes it.
pub fn codec_header_pcm(f: Format) -> Vec<u8> {
    let mut riff = Vec::with_capacity(44);
    riff.extend_from_slice(b"RIFF");
    riff.extend_from_slice(&36u32.to_le_bytes());
    riff.extend_from_slice(b"WAVE");
    riff.extend_from_slice(b"fmt ");
    riff.extend_from_slice(&16u32.to_le_bytes());
    riff.extend_from_slice(&1u16.to_le_bytes());
    riff.extend_from_slice(&f.channels.to_le_bytes());
    riff.extend_from_slice(&f.rate.to_le_bytes());
    riff.extend_from_slice(&(f.rate * u32::from(f.bits) * u32::from(f.channels) / 8).to_le_bytes());
    riff.extend_from_slice(&(f.channels * f.bits.div_ceil(8)).to_le_bytes());
    riff.extend_from_slice(&f.bits.to_le_bytes());
    riff.extend_from_slice(b"data");
    riff.extend_from_slice(&0u32.to_le_bytes());
    let mut out = Vec::with_capacity(4 + 3 + 4 + riff.len());
    put_str(&mut out, b"pcm");
    put_str(&mut out, &riff);
    out
}

/// The format a codec header says, when it is plain PCM.
pub fn read_codec_header(payload: &[u8]) -> Option<Format> {
    let (codec, rest) = take_str(payload)?;
    if codec != b"pcm" {
        return None;
    }
    let (riff, _) = take_str(rest)?;
    if riff.len() < 36 || &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" || &riff[12..16] != b"fmt " || u16_at(riff, 20) != 1 {
        return None;
    }
    let f = Format { channels: u16_at(riff, 22), rate: u32_at(riff, 24), bits: u16_at(riff, 34) };
    (f.rate > 0 && f.channels > 0 && matches!(f.bits, 16 | 24 | 32)).then_some(f)
}

/// A wire chunk's payload: when it was captured (µs, the server's clock; it sounds a buffer later) and its audio.
pub fn wire_chunk(stamp_us: i64, audio: &[u8]) -> Vec<u8> {
    let (s, u) = tv(stamp_us);
    let mut out = Vec::with_capacity(12 + audio.len());
    out.extend_from_slice(&s.to_le_bytes());
    out.extend_from_slice(&u.to_le_bytes());
    put_str(&mut out, audio);
    out
}

/// A wire chunk's audio.
pub fn read_wire_chunk(payload: &[u8]) -> Option<&[u8]> {
    take_str(payload.get(8..)?).map(|(audio, _)| audio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_message_reads_back_as_written_with_its_arrival_stamped() {
        let payload = time(-1_500);
        let bytes = message(TIME, 7, 3, 12_345_678_901, &payload);
        assert_eq!(bytes.len(), BASE + 8);
        let (h, p) = read(&mut bytes.as_slice(), || 42).await.unwrap();
        assert_eq!(h, Header { kind: TIME, id: 7, refers_to: 3, sent_us: 12_345_678_901, received_us: 42, size: 8 });
        // A negative latency as snapcast keeps it: seconds rounded down, microseconds after them.
        assert_eq!(i32_at(&p, 0), -1);
        assert_eq!(i32_at(&p, 4), 998_500);
    }

    #[test]
    fn the_pcm_header_is_snapservers() {
        let f = Format { rate: 44_100, bits: 16, channels: 2 };
        let h = codec_header_pcm(f);
        assert_eq!(&h[0..4], &3u32.to_le_bytes());
        assert_eq!(&h[4..7], b"pcm");
        assert_eq!(&h[7..11], &44u32.to_le_bytes());
        let riff = &h[11..];
        assert_eq!(u32_at(riff, 28), 176_400, "bytes per second");
        assert_eq!(u16_at(riff, 32), 4, "block align");
        assert_eq!(read_codec_header(&h), Some(f));
        assert_eq!(f.duration_us(176_400), 1_000_000);
        assert_eq!(f.duration_us(3), 0, "less than a frame plays no time");
    }

    #[test]
    fn a_wire_chunk_carries_its_stamp_and_audio() {
        let c = wire_chunk(3_000_250_000, &[1, 2, 3, 4]);
        assert_eq!(i32_at(&c, 0), 3_000);
        assert_eq!(i32_at(&c, 4), 250_000);
        assert_eq!(read_wire_chunk(&c), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(read_wire_chunk(&c[..10]), None);
    }

    #[test]
    fn json_payloads_read_back() {
        let s = server_settings(1000, 0, 140, true);
        let v = read_json(&s).unwrap();
        assert_eq!(v["bufferMs"], 1000);
        assert_eq!(v["volume"], 100);
        assert_eq!(v["muted"], true);
        assert_eq!(read_json(&[9, 0, 0, 0, b'{']), None);
    }
}
