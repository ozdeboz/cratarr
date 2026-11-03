// The tape head's audio worklet (decision 0102, as amended): the reading of lib/tape/head on the audio thread, as the
// app's reader thread does it. It keeps the playing track's chunks itself and hands each one it wants to an idle
// decoding worker (lib/tape/decoder.worker, over the ports it was given), and tells the page what each block was read
// from (its marks) and when a reading ended. Its motion is the engine's speed, ms of its own time in.
import { Chunks, Reader, type Chunk, type Ended } from './head';

// The worklet's own scope (not in the DOM's types).
declare const sampleRate: number;
declare const currentFrame: number;
declare function registerProcessor(name: string, processor: new (options?: unknown) => object): void;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}

/** A motion as the page sends it: a steady speed (× the track's, signed) until the engine's motions come (0102). */
export type Motion = { kind: 'steady'; v: number };
const speedOf = (m: Motion): ((tMs: number) => number) => {
  const v = m.v;
  return () => v;
};

/** Blocks between the marks sent to the page. */
const MARKS_EVERY = 4;

class TapeHeadProcessor extends AudioWorkletProcessor {
  private chunks: Chunks | null = null;
  private rate = 0;
  private reader: Reader | null = null;
  /** The context's frame its reading began at: its marks are sent as the context's frames. */
  private began = 0;
  private workers: { port: MessagePort; busy: boolean }[] = [];
  private pending: { frame: number; at: number; v: number }[] = [];
  private blocks = 0;

  constructor() {
    super();
    this.port.onmessage = (e: MessageEvent) => this.told(e.data);
  }

  private told(m: { type: string; [k: string]: unknown }) {
    switch (m.type) {
      case 'track':
        this.reader = null;
        this.chunks = new Chunks(m.frames as number);
        this.rate = m.rate as number;
        for (const w of this.workers) w.port.close();
        this.workers = (m.ports as MessagePort[]).map((port) => {
          const w = { port, busy: false };
          port.onmessage = (q: MessageEvent) => {
            w.busy = false;
            const { k, chunk } = q.data as { k: number; chunk: Chunk };
            this.chunks?.put(k, chunk);
            this.dispatch();
          };
          return w;
        });
        break;
      case 'start':
        if (!this.chunks) return;
        this.reader = new Reader({ chunks: this.chunks, rate: this.rate, outRate: sampleRate, fromMs: m.fromMs as number, speed: speedOf(m.motion as Motion), gain: m.gain as number });
        // Its first block's frame, taken as it fills it.
        this.began = -1;
        this.pending = [];
        this.dispatch();
        break;
      case 'stop':
        this.reader?.stop();
        break;
    }
  }

  /** Each idle worker given the nearest chunk wanted. */
  private dispatch() {
    if (!this.chunks) return;
    for (const w of this.workers) {
      if (w.busy) continue;
      const k = this.chunks.take();
      if (k === undefined) return;
      w.busy = true;
      w.port.postMessage({ k });
    }
  }

  private ended(why: Ended) {
    this.port.postMessage({ type: 'marks', marks: this.pending, misses: this.chunks?.misses ?? 0 });
    this.port.postMessage({ type: 'ended', why });
    this.reader = null;
    this.pending = [];
  }

  process(_inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
    const r = this.reader;
    const out = outputs[0];
    if (!r || out.length < 2) return true;
    if (this.began < 0) this.began = currentFrame;
    const why = r.fill(out[0], out[1]);
    const m = r.marks.last();
    if (m) this.pending.push({ frame: this.began + m.frame, at: m.at, v: m.v });
    this.dispatch();
    if (why) this.ended(why);
    else if (++this.blocks % MARKS_EVERY === 0) {
      this.port.postMessage({ type: 'marks', marks: this.pending });
      this.pending = [];
    }
    return true;
  }
}

registerProcessor('cratarr-tape-head', TapeHeadProcessor);
