// One of the tape head's decoding workers (decision 0102, as amended; the app's Chunks' workers): the playing track's
// file opened on a decoder of its own (decode.wasm, lib/tape/decoder), and each chunk the head asks for decoded and
// handed to it straight (over the port it was given), its buffer moved, not copied.
import decodeUrl from '../wasm/decode.wasm?url';
import { CHUNK } from './head';
import { openDecoder, type TapeDecoder } from './decoder';

/** decode.wasm compiled once for this worker; none where it cannot be (a WAV still opens). */
const module: Promise<WebAssembly.Module | null> = fetch(decodeUrl)
  .then((r) => r.arrayBuffer())
  .then((b) => WebAssembly.compile(b))
  .catch(() => null);

let decoder: TapeDecoder | null = null;
let head: MessagePort | null = null;
const post = (m: unknown) => (self as unknown as Worker).postMessage(m);

self.onmessage = async (e: MessageEvent) => {
  const m = e.data;
  if (m?.type !== 'open') return;
  decoder?.close();
  head?.close();
  decoder = null;
  const port = m.port as MessagePort;
  head = port;
  try {
    decoder = await openDecoder(m.bytes as ArrayBuffer, await module);
    post({ type: 'opened', id: m.id, rate: decoder.rate, frames: decoder.frames, by: decoder.by });
  } catch (err) {
    post({ type: 'failed', id: m.id, error: err instanceof Error ? err.message : String(err) });
    return;
  }
  port.onmessage = (q: MessageEvent) => {
    const k = q.data?.k;
    if (typeof k !== 'number' || !decoder) return;
    let chunk: Int16Array;
    try {
      chunk = decoder.read(k * CHUNK, CHUNK);
    } catch {
      chunk = new Int16Array(0);
    }
    port.postMessage({ k, chunk }, [chunk.buffer]);
  };
};
