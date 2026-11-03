// One of the tape head's decoding workers (decision 0102, as amended; the app's Chunks' workers): the playing track's
// file opened on a decoder of its own, and each chunk the head asks for decoded and handed to it straight (over the
// port it was given), its buffer moved, not copied.
import { CHUNK } from './head';
import { openDecoder, type TapeDecoder } from './decoder';

let decoder: TapeDecoder | null = null;
let head: MessagePort | null = null;

self.onmessage = (e: MessageEvent) => {
  const m = e.data;
  if (m?.type !== 'open') return;
  decoder?.close();
  head?.close();
  decoder = null;
  head = m.port as MessagePort;
  try {
    decoder = openDecoder(m.bytes as ArrayBuffer);
    (self as unknown as Worker).postMessage({ type: 'opened', id: m.id, rate: decoder.rate, frames: decoder.frames });
  } catch (err) {
    (self as unknown as Worker).postMessage({ type: 'failed', id: m.id, error: err instanceof Error ? err.message : String(err) });
    return;
  }
  head.onmessage = (q: MessageEvent) => {
    const k = q.data?.k;
    if (typeof k !== 'number' || !decoder || !head) return;
    let chunk: Int16Array;
    try {
      chunk = decoder.read(k * CHUNK, CHUNK);
    } catch {
      chunk = new Int16Array(0);
    }
    head.postMessage({ k, chunk }, [chunk.buffer]);
  };
};
