// A track's file opened for the tape head's chunks (decision 0102, as amended): any stretch of it decoded on request,
// 16-bit stereo interleaved, as the app's TapeDecoder reads (symphonia, through a5's Rust bridge; the page's as
// WebAssembly once it is built). Until then, and for the tests, a plain WAV file opens here too.

export type TapeDecoder = {
  /** Frames a second. */
  rate: number;
  /** The track's length (frames). */
  frames: number;
  /** `count` frames from `frame` on, 16-bit stereo interleaved (a mono file's channel twice); fewer at its end. */
  read(frame: number, count: number): Int16Array;
  close(): void;
};

/** Opens a file's bytes: the WebAssembly decoder's (when the page has it), else a WAV's own reading. */
export function openDecoder(bytes: ArrayBuffer): TapeDecoder {
  return wav(bytes);
}

/** A plain WAV (PCM of 8, 16, 24 or 32 bits, or 32-bit float; mono or stereo, of more channels the first two). */
export function wav(bytes: ArrayBuffer): TapeDecoder {
  const v = new DataView(bytes);
  const tag = (at: number) => String.fromCharCode(v.getUint8(at), v.getUint8(at + 1), v.getUint8(at + 2), v.getUint8(at + 3));
  if (bytes.byteLength < 12 || tag(0) !== 'RIFF' || tag(8) !== 'WAVE') throw new Error('not a WAV file');
  let fmt: { format: number; channels: number; rate: number; bits: number } | null = null;
  let data = -1;
  let length = 0;
  for (let at = 12; at + 8 <= bytes.byteLength; ) {
    const size = v.getUint32(at + 4, true);
    if (tag(at) === 'fmt ') fmt = { format: v.getUint16(at + 8, true), channels: v.getUint16(at + 10, true), rate: v.getUint32(at + 12, true), bits: v.getUint16(at + 22, true) };
    if (tag(at) === 'data') {
      data = at + 8;
      length = Math.min(size, bytes.byteLength - data);
      break;
    }
    at += 8 + size + (size & 1);
  }
  if (!fmt || data < 0) throw new Error('a WAV file without its format or its sound');
  const { format, channels, rate, bits } = fmt;
  const float = format === 3;
  if (format !== 1 && format !== 3) throw new Error(`a WAV file of format ${format}`);
  const step = bits / 8;
  const frameBytes = step * channels;
  const frames = Math.floor(length / frameBytes);
  const sampleAt = (at: number): number => {
    if (float) return Math.max(-1, Math.min(1, v.getFloat32(at, true))) * 32767;
    switch (bits) {
      case 8:
        return (v.getUint8(at) - 128) << 8;
      case 16:
        return v.getInt16(at, true);
      case 24:
        return ((v.getUint8(at) | (v.getUint8(at + 1) << 8) | (v.getInt8(at + 2) << 16)) >> 8);
      case 32:
        return v.getInt32(at, true) >> 16;
      default:
        throw new Error(`a WAV file of ${bits} bits`);
    }
  };
  return {
    rate,
    frames,
    read(frame, count) {
      const n = Math.max(0, Math.min(count, frames - frame));
      const out = new Int16Array(n * 2);
      for (let i = 0; i < n; i++) {
        const at = data + (frame + i) * frameBytes;
        const l = sampleAt(at);
        out[i * 2] = l;
        out[i * 2 + 1] = channels > 1 ? sampleAt(at + step) : l;
      }
      return out;
    },
    close() {},
  };
}
