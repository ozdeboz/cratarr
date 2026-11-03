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

/** A motion as the page sends it, the engine's (tape.wasm, the rules crate's tape: a5's): a tap's, a hold's, or a
 *  steady speed (× the track's, signed). */
export type Motion =
  | { kind: 'steady'; v: number }
  | { kind: 'tap'; way: number; length: number; attack: number; release: number; floor: number; sign: number }
  | { kind: 'hold'; base: number; top: number; doubling: number; sign: number };

/** tape.wasm's own calls: a motion is a handle (0: no free slot), its speed over its own time. */
type TapeWasm = {
  motion_tap(way: number, length: number, attack: number, release: number, floor: number, sign: number): number;
  motion_hold(base: number, top: number, doubling: number, sign: number): number;
  motion_steady(v: number): number;
  motion_let_go(h: number, atMs: number, to: number, brakeMs: number): void;
  motion_speed(h: number, tMs: number): number;
  motion_gone(h: number, tMs: number): number;
  motion_stands_at(h: number): number;
  motion_free(h: number): void;
};
let engine: TapeWasm | null = null;

/** A motion made in the engine: its handle, or none where there is no engine (a steady speed reads on without one). */
function made(m: Motion): number {
  if (!engine) return 0;
  switch (m.kind) {
    case 'tap':
      return engine.motion_tap(m.way, m.length, m.attack, m.release, m.floor, m.sign);
    case 'hold':
      return engine.motion_hold(m.base, m.top, m.doubling, m.sign);
    case 'steady':
      return engine.motion_steady(m.v);
  }
}

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
  /** The reading's motion in the engine (0: none), and where in the track it began (ms). */
  private motion = 0;
  private fromMs = 0;

  constructor() {
    super();
    this.port.onmessage = (e: MessageEvent) => this.told(e.data);
  }

  private told(m: { type: string; [k: string]: unknown }) {
    switch (m.type) {
      case 'engine':
        void WebAssembly.instantiate(m.bytes as ArrayBuffer).then(
          (r) => {
            engine = r.instance.exports as unknown as TapeWasm;
            this.port.postMessage({ type: 'engine', ready: true });
          },
          (e) => this.port.postMessage({ type: 'engine', ready: false, error: String(e) }),
        );
        break;
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
      case 'start': {
        if (!this.chunks) return;
        this.letGoOfMotion();
        const motion = m.motion as Motion;
        const h = made(motion);
        this.motion = h;
        this.fromMs = m.fromMs as number;
        const steady = motion.kind === 'steady' ? motion.v : 0;
        const x = engine;
        const speed = h && x ? (t: number) => x.motion_speed(h, t) : () => steady;
        this.reader = new Reader({ chunks: this.chunks, rate: this.rate, outRate: sampleRate, fromMs: m.fromMs as number, speed, gain: m.gain as number });
        // Its first block's frame, taken as it fills it.
        this.began = -1;
        this.pending = [];
        this.dispatch();
        break;
      }
      // What changes a reading under way: its motion let go, braking from its own speed at `atMs` (now, by the frames
      // it read, where none is given) to `to` × (the engine's brake, `brakeMs` at least); the page is told when (ms of
      // its own time) and where in the track (ms) it will stand.
      case 'let-go': {
        if (!this.motion || !engine || !this.reader) return;
        const at = typeof m.atMs === 'number' ? m.atMs : (this.reader.written * 1000) / sampleRate;
        engine.motion_let_go(this.motion, at, m.to as number, m.brakeMs as number);
        const when = engine.motion_stands_at(this.motion);
        this.port.postMessage({ type: 'stands-at', when, place: when < 0 ? -1 : this.fromMs + engine.motion_gone(this.motion, when) });
        break;
      }
      case 'stop':
        this.reader?.stop();
        break;
    }
  }

  private letGoOfMotion() {
    if (this.motion && engine) engine.motion_free(this.motion);
    this.motion = 0;
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
    this.letGoOfMotion();
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
