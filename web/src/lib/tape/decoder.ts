// A track's file opened for the tape head's chunks (decision 0102, as amended): any stretch of it decoded on request,
// 16-bit stereo interleaved, by the same decoder as the app's (a5's server/tape-decode, symphonia; the page's as
// decode.wasm: no imports, the file's bytes copied into its memory, read on without seeking where it goes on). A plain
// WAV file opens here too where the decoder cannot take a file.

export type TapeDecoder = {
  /** Which decoder opened it (the log's). */
  by: 'decode.wasm' | 'wav';
  /** Frames a second. */
  rate: number;
  /** The track's length (frames). */
  frames: number;
  /** `count` frames from `frame` on, 16-bit stereo interleaved (a mono file's channel twice); fewer at its end. */
  read(frame: number, count: number): Int16Array;
  close(): void;
};

/** decode.wasm's own calls (a5's). */
type DecodeWasm = {
  memory: WebAssembly.Memory;
  alloc(len: number): number;
  free(ptr: number, len: number): void;
  open(ptr: number, len: number): number;
  rate(h: number): number;
  frames(h: number): number;
  read(h: number, from: number, count: number): number;
  read_len(): number;
  close(h: number): void;
};

/** Opens a file's bytes with decode.wasm (`module`, compiled once), else as a WAV's own reading. */
export async function openDecoder(bytes: ArrayBuffer, module: WebAssembly.Module | null): Promise<TapeDecoder> {
  if (module) {
    try {
      return await decoded(bytes, module);
    } catch (e) {
      try {
        return wav(bytes);
      } catch {
        throw e;
      }
    }
  }
  return wav(bytes);
}

/** The file in decode.wasm: an instance of its own (a worker's), the bytes copied into its memory and opened there. */
export async function decoded(bytes: ArrayBuffer, module: WebAssembly.Module): Promise<TapeDecoder> {
  const x = (await WebAssembly.instantiate(module)).exports as unknown as DecodeWasm;
  const len = bytes.byteLength;
  const ptr = x.alloc(len);
  new Uint8Array(x.memory.buffer, ptr, len).set(new Uint8Array(bytes));
  // Open takes the room; where it cannot decode the file, the room is given back.
  const h = x.open(ptr, len);
  if (!h) {
    x.free(ptr, len);
    throw new Error('the decoder cannot read this file');
  }
  return {
    by: 'decode.wasm',
    rate: x.rate(h),
    frames: Number(x.frames(h)),
    read(frame, count) {
      const p = x.read(h, frame, count);
      const n = x.read_len();
      // Valid until the next read: copied out (and so movable to the head).
      return new Int16Array(x.memory.buffer.slice(p, p + n));
    },
    close() {
      x.close(h);
    },
  };
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
    by: 'wav',
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
