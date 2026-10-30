// An MP3's frames as its headers tell them, without decoding (the owner, 2026-10-25: "light device waveform analysis"
// for a Bandcamp preview, which has no analysis of the server's): where each frame starts, and what the stream is
// (its sample rate, channels, bitrate), so the device decodes it a few seconds at a time for the waveform and shows
// what it plays. Pure: bytes in, frames out.

export type Mp3 = {
  /** Each frame's first byte. */
  starts: number[];
  /** The end of the last frame. */
  end: number;
  sampleRate: number;
  channels: number;
  /** The mean over the frames (a VBR file's too). */
  bitrateKbps: number;
  samplesPerFrame: number;
};

const RATES_V1 = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
const RATES_V2 = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
const HZ = [44100, 48000, 32000];

type Header = { length: number; sampleRate: number; channels: number; kbps: number; samples: number };

/** A Layer III frame's header at `at`, else null. */
function header(b: Uint8Array, at: number): Header | null {
  if (at + 4 > b.length || b[at] !== 0xff || (b[at + 1] & 0xe0) !== 0xe0) return null;
  const version = (b[at + 1] >> 3) & 3; // 3: MPEG-1, 2: MPEG-2, 0: MPEG-2.5
  const layer = (b[at + 1] >> 1) & 3; // 1: Layer III
  const rate = b[at + 2] >> 4;
  const hz = (b[at + 2] >> 2) & 3;
  if (version === 1 || layer !== 1 || rate === 0 || rate === 15 || hz === 3) return null;
  const v1 = version === 3;
  const kbps = (v1 ? RATES_V1 : RATES_V2)[rate];
  const sampleRate = HZ[hz] / (v1 ? 1 : version === 2 ? 2 : 4);
  const padding = (b[at + 2] >> 1) & 1;
  const length = Math.floor(((v1 ? 144 : 72) * kbps * 1000) / sampleRate) + padding;
  return { length, sampleRate, channels: (b[at + 3] >> 6) === 3 ? 1 : 2, kbps, samples: v1 ? 1152 : 576 };
}

/** The first frame from `from` on: a header whose next frame has one too, as the server's first_frame finds it (a
 *  stray 0xFF, or bytes that look like a header by chance, are no frame; 0.10: a seek's stream, taken from a false
 *  one at 11025 Hz, came out short and was passed over). Null while the bytes are too few to say. */
export function firstFrame(b: Uint8Array, from = 0): number | null {
  for (let at = from; at + 4 <= b.length; at++) {
    const h = header(b, at);
    if (!h) continue;
    if (at + h.length + 4 > b.length) return null;
    if (header(b, at + h.length)) return at;
  }
  return null;
}

/** The frames of an MP3 (an ID3v2 tag before them passed over), else null if it has too few to be one. */
export function frames(b: Uint8Array): Mp3 | null {
  let at = 0;
  if (b[0] === 0x49 && b[1] === 0x44 && b[2] === 0x33 && b.length >= 10) {
    const size = ((b[6] & 0x7f) << 21) | ((b[7] & 0x7f) << 14) | ((b[8] & 0x7f) << 7) | (b[9] & 0x7f);
    at = 10 + size + (b[5] & 0x10 ? 10 : 0);
  }
  // The first frame: a header whose next frame has one too (a stray 0xFF is no frame).
  let first: Header | null = null;
  for (; at + 4 <= b.length; at++) {
    const h = header(b, at);
    if (h && header(b, at + h.length)) {
      first = h;
      break;
    }
  }
  if (!first) return null;
  const starts: number[] = [];
  let kbps = 0;
  for (let h = header(b, at); h && at + h.length <= b.length; h = header(b, at)) {
    starts.push(at);
    kbps += h.kbps;
    at += h.length;
  }
  if (starts.length < 2) return null;
  return { starts, end: at, sampleRate: first.sampleRate, channels: first.channels, bitrateKbps: Math.round(kbps / starts.length), samplesPerFrame: first.samples };
}

/** Which frames each of `n` bins covers, as [from, to) frame numbers. */
export function binFrames(count: number, n: number): [number, number][] {
  return Array.from({ length: n }, (_, i) => [Math.floor((i * count) / n), Math.max(Math.floor((i * count) / n) + 1, Math.floor(((i + 1) * count) / n))]);
}

/** Bins' RMS as the waveform keeps them: 0..=255 of the loudest. */
export function scaled(rms: number[]): Uint8Array {
  const top = Math.max(...rms, 1e-9);
  return Uint8Array.from(rms, (x) => Math.round((Math.min(x, top) / top) * 255));
}
