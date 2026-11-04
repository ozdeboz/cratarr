// The tape head's audio worklet (decision 0102, as amended): the reading of lib/tape/head on the audio thread, as the
// app's reader thread does it. It keeps the playing track's chunks itself and hands each one it wants to an idle
// decoding worker (lib/tape/decoder.worker, over the ports it was given), and tells the page what each block was read
// from (its marks) and when a reading ended. Its motion is the engine's speed, ms of its own time in.
import { Chunks, Reader, madeIn, type Chunk, type Ended, type Motion, type TapeWasm } from './head';

// The worklet's own scope (not in the DOM's types).
declare const sampleRate: number;
declare const currentFrame: number;
declare function registerProcessor(name: string, processor: new (options?: unknown) => object): void;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}

let engine: TapeWasm | null = null;

/** A motion made in the engine: its handle, or none where there is no engine (a steady speed reads on without one). */
const made = (m: Motion): number => (engine ? madeIn(engine, m) : 0);

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
  /** The reading's motion in the engine (0: none); where in the track it began (ms), at which of its motion's ms (a
   *  motion carried on from the track before starts part-way); the motion's ms it has done what it is for at (a tap
   *  landed, a spin-up at its speed, a let-go at its new speed; -1: never, a hold or a steady not let go), and whether
   *  the page was told. */
  private motion = 0;
  private fromMs = 0;
  private motionAt = 0;
  private doneAt = -1;
  private doneTold = false;

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
        this.motionAt = typeof m.motionAt === 'number' ? m.motionAt : 0;
        this.doneTold = false;
        // A motion carried on from the track before, let go there: let go the same here.
        const lg = m.letGo as { atMs: number; to: number; brakeMs: number } | undefined;
        if (h && engine && lg) engine.motion_let_go(h, lg.atMs, lg.to, lg.brakeMs);
        this.doneAt = h && engine ? engine.motion_done_at(h) : -1;
        const steady = motion.kind === 'steady' ? motion.v : 0;
        const x = engine;
        const t0 = this.motionAt;
        const speed = h && x ? (t: number) => x.motion_speed(h, t0 + t) : () => steady;
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
        const at = typeof m.atMs === 'number' ? m.atMs : this.motionAt + this.reader.ms;
        engine.motion_let_go(this.motion, at, m.to as number, m.brakeMs as number);
        const when = engine.motion_stands_at(this.motion);
        this.doneAt = engine.motion_done_at(this.motion);
        this.doneTold = false;
        const gone = when < 0 ? 0 : engine.motion_gone(this.motion, when) - engine.motion_gone(this.motion, this.motionAt);
        this.port.postMessage({ type: 'stands-at', when, atMs: at, place: when < 0 ? -1 : this.fromMs + gone });
        break;
      }
      // Its gain from now on (the page's volume, ducking, the track's level), followed without a click.
      case 'gain':
        this.reader?.setGain(m.gain as number);
        break;
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
    // Where its motion had got (its ms), for the page to carry it on into the next track.
    this.port.postMessage({ type: 'ended', why, motionMs: this.motionAt + (this.reader?.ms ?? 0) });
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
    // Done what it is for (landed, at its speed, at its let-go's): the page told the frame it reaches the ear at.
    if (this.doneAt >= 0 && !this.doneTold && this.motionAt + r.ms >= this.doneAt) {
      this.doneTold = true;
      this.port.postMessage({ type: 'done', frame: this.began + r.written });
    }
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
