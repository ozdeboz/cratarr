// The tape's head on the page (decision 0102, the owner 2026-11-03; amended the same day after measuring the tablet and
// reading Mixxx and xwax): the reading itself, the same as the app's (android TapeHead.kt, its constants and its
// behaviour, a5's), pure, so the page's audio worklet (lib/tape/head.worklet) and the tests run the same code.
//
// - **Chunks on demand** ([Chunks], Mixxx's CachingReader): [CHUNK] frames each, decoded by workers as the head asks
//   for them, [KEPT] of them kept around where the tape is, the least recently read let go. The head asks ahead the way
//   it goes, as far as its speed needs ([ahead]); above [TOP] only the chunks under each coming piece. A chunk not there
//   yet is silence with a short fade out and in, never a wait.
// - **Its motion** is the engine's (`speed`: the track's own speed, signed, ms of the head's own time in). Faster than
//   [TOP] it is heard in pieces ([PIECE_MS], crossfaded), each read at the top from where the head has got.
// - **Its volume follows its speed below a quarter** (the owner, after xwax): full down to [QUIET_BELOW], then with the
//   speed to silence at a stand.
// - **Where it is heard** ([Marks]): from the frames the output was given, through what each block was read from, never
//   from the clock (2026-11-03, the owner: "No guessing anywhere").
// The page's output runs at its own rate (the context's), the track at its own: a frame given is `rate / outRate` of
// the track's at speed 1; times are the output's.

/** A motion as the page sends it, the engine's (tape.wasm, the rules crate's tape: a5's): a tap's, a hold's (flown
 *  from 1×, or spun up from a stand over `attack` ms), a spin-up from a stand to `v` × over `attack` ms, or a steady
 *  speed (× the track's, signed). */
export type Motion =
  | { kind: 'steady'; v: number }
  | { kind: 'rise'; v: number; attack: number }
  | { kind: 'tap'; way: number; length: number; attack: number; release: number; floor: number; sign: number }
  | { kind: 'hold'; base: number; top: number; doubling: number; sign: number }
  | { kind: 'hold-from-still'; base: number; top: number; doubling: number; attack: number; sign: number };

/** tape.wasm's own calls: a motion is a handle (0: no free slot), its speed over its own time. */
export type TapeWasm = {
  motion_tap(way: number, length: number, attack: number, release: number, floor: number, sign: number): number;
  motion_hold(base: number, top: number, doubling: number, sign: number): number;
  motion_steady(v: number): number;
  motion_rise(v: number, attack: number): number;
  motion_hold_from_still(base: number, top: number, doubling: number, attack: number, sign: number): number;
  motion_let_go(h: number, atMs: number, to: number, brakeMs: number): void;
  motion_speed(h: number, tMs: number): number;
  motion_gone(h: number, tMs: number): number;
  motion_stands_at(h: number): number;
  motion_done_at(h: number): number;
  motion_free(h: number): void;
};

/** A motion made in the engine `x`: its handle (0: no free slot). */
export function madeIn(x: TapeWasm, m: Motion): number {
  switch (m.kind) {
    case 'tap':
      return x.motion_tap(m.way, m.length, m.attack, m.release, m.floor, m.sign);
    case 'hold':
      return x.motion_hold(m.base, m.top, m.doubling, m.sign);
    case 'hold-from-still':
      return x.motion_hold_from_still(m.base, m.top, m.doubling, m.attack, m.sign);
    case 'rise':
      return x.motion_rise(m.v, m.attack);
    case 'steady':
      return x.motion_steady(m.v);
  }
}

/** Frames a chunk (Mixxx's 8192: about 0.19 s at 44.1 kHz). */
export const CHUNK = 8192;
/** Chunks kept (about 45 s, 8 MB at 16-bit stereo). */
export const KEPT = 256;
/** Workers decoding chunks, each with its own decoder. */
export const WORKERS = 2;
/** Chunks asked and not yet decoded, at most (the farthest dropped). */
export const ASKED_MOST = 128;
/** How far ahead a head asks (ms of its own time, at its speed). */
export const LOOKAHEAD_MS = 600;
/** Pieces ahead asked for above the top. */
export const PIECES_AHEAD = 2;
/** The fastest a head is heard reading (×): above it, pieces. */
export const TOP = 16;
/** A piece's length heard above the top (ms), as the server's streams were. */
export const PIECE_MS = 350;
/** A piece's crossfade (ms). */
export const XFADE_MS = 12;
/** The fade in and out of a start, a stop and a missing chunk (ms). */
export const FADE_MS = 8;
/** Below this speed (×) its volume follows its speed. */
export const QUIET_BELOW = 0.25;
/** Marks kept (blocks). */
export const MARKS = 4096;

/** A chunk: its frames as 16-bit stereo, interleaved (the decoder's, as the app's TapeDecoder reads them). */
export type Chunk = Int16Array;

/** Catmull-Rom between `y1` and `y2`, `t` of the way (the app's Varispeed.cubic). */
export const cubic = (y0: number, y1: number, y2: number, y3: number, t: number): number =>
  y1 + 0.5 * t * (y2 - y0 + t * (2 * y0 - 5 * y1 + 4 * y2 - y3 + t * (3 * (y1 - y2) + y3 - y0)));

/**
 * The chunks the head will read next: from `at` (the track's frames) the way `v` goes, as far as its speed needs; above
 * the top only those under the piece playing and each coming one. `rate`: the track's; `tMs`: the head's own time.
 */
export function ahead(at: number, v: number, tMs: number, rate: number): number[] {
  const here = Math.floor(at / CHUNK);
  const sign = v < 0 ? -1 : 1;
  if (Math.abs(v) <= TOP) {
    const n = Math.min(ASKED_MOST, Math.max(2, Math.ceil(((Math.max(1, Math.abs(v)) * LOOKAHEAD_MS) / 1000) * rate / CHUNK)));
    return Array.from({ length: n + 1 }, (_, i) => here + sign * i);
  }
  // Pieces: the one playing, from where it began, and the next: each reads TOP × PIECE_MS from where the head is as it
  // begins.
  const perPiece = Math.ceil(((TOP * PIECE_MS) / 1000) * rate / CHUNK) + 1;
  const next = Math.floor(tMs / PIECE_MS);
  const out = [here, here + sign];
  for (let p = 0; p < PIECES_AHEAD; p++) {
    const begins = at + (v * ((next + p) * PIECE_MS - tMs) * rate) / 1000;
    const k = Math.floor(begins / CHUNK);
    for (let j = 0; j < perPiece; j++) out.push(k + sign * j);
  }
  return out;
}

/**
 * A track's chunks: those wanted nearest first ([want]), handed to the workers one at a time ([take]), kept as they
 * come ([put]), the least recently read let go past [KEPT], read without waiting ([get]: none while not there).
 */
export class Chunks {
  /** The track's length (frames). */
  readonly frames: number;
  /** Kept, the least recently read first. */
  private kept = new Map<number, Chunk>();
  private asked: number[] = [];
  private taken = new Set<number>();
  /** Blocks that found a chunk missing (the log's). */
  misses = 0;

  constructor(frames: number) {
    this.frames = frames;
  }

  /** Chunks wanted, nearest first: these first, in their order, then what was asked before; the farthest dropped past
   *  [ASKED_MOST] (dropping the nearest left a 64× hold's pieces silent, the app's, 2026-11-04). */
  want(ks: number[]) {
    const now = [...new Set(ks.filter((k) => k >= 0 && k * CHUNK < this.frames && !this.kept.has(k) && !this.taken.has(k)))];
    const fresh = new Set(now);
    this.asked = [...now, ...this.asked.filter((k) => !fresh.has(k))].slice(0, ASKED_MOST);
  }

  /** The nearest asked, for a worker to decode; none while nothing is asked. */
  take(): number | undefined {
    while (this.asked.length) {
      const k = this.asked.shift()!;
      if (this.kept.has(k) || this.taken.has(k)) continue;
      this.taken.add(k);
      return k;
    }
    return undefined;
  }

  /** A chunk decoded (empty: it could not be). */
  put(k: number, chunk: Chunk) {
    this.taken.delete(k);
    this.kept.delete(k);
    this.kept.set(k, chunk);
    while (this.kept.size > KEPT) this.kept.delete(this.kept.keys().next().value!);
  }

  /** A chunk as kept (read: the most recent); none while it is not decoded. */
  get(k: number): Chunk | undefined {
    const c = this.kept.get(k);
    if (c) {
      this.kept.delete(k);
      this.kept.set(k, c);
    }
    return c;
  }

  get size() {
    return this.kept.size;
  }
}

/** A block given to the output: the output's frame it began at, where in the track (frames) it read from, how fast. */
export type Mark = { frame: number; at: number; v: number };

/** What each block given to the output was read from, so where the sound heard is can be read off the output's own
 *  count of frames played. */
export class Marks {
  private list: Mark[] = [];
  private readonly ratio: number;
  private readonly rate: number;

  /** `rate`: the track's; `outRate`: the output's. */
  constructor(rate: number, outRate: number) {
    this.rate = rate;
    this.ratio = rate / outRate;
  }

  add(m: Mark) {
    this.list.push(m);
    if (this.list.length > MARKS) this.list.splice(0, this.list.length - MARKS);
  }

  /** The latest block's. */
  last(): Mark | undefined {
    return this.list[this.list.length - 1];
  }

  clear() {
    this.list = [];
  }

  private at(played: number): Mark | undefined {
    for (let i = this.list.length - 1; i >= 0; i--) if (this.list[i].frame <= played) return this.list[i];
    return undefined;
  }

  /** Where the sound heard now is (ms of the track), the output having played `played` frames; none before any. */
  heardMs(played: number): number | undefined {
    const m = this.at(played);
    return m && ((m.at + m.v * this.ratio * (played - m.frame)) * 1000) / this.rate;
  }

  /** How fast the sound heard now was read (× the track's, signed); none before any. */
  heardSpeed(played: number): number | undefined {
    return this.at(played)?.v;
  }
}

/** Why a reading ended: stopped, or past the track's start or end. */
export type Ended = 'stopped' | 'start' | 'end';

/**
 * One reading of a track from a place at the engine's speed: [fill] gives the output its next frames, as the app's
 * read loop does for each of its blocks; [stop] fades it out. `speed`: × the track's, signed, ms of its own time in.
 */
export class Reader {
  private readonly chunks: Chunks;
  private readonly speed: (tMs: number) => number;
  /** The gain asked for, and the one sounding: it follows the asked one over FADE_MS, so a change never clicks. */
  private gainWanted: number;
  private gainNow: number;
  private readonly rate: number;
  private readonly outRate: number;
  private readonly ratio: number;
  private readonly fadeStep: number;
  private readonly xfade: number;
  private readonly frames: number;
  /** Where it reads (the track's frames). */
  at: number;
  /** Frames given to the output since it started. */
  written = 0;
  private stopping = false;
  private reason: Ended = 'stopped';
  private piece = -1;
  private pieceFrom = 0;
  private fade = 0;
  /** A missing chunk's silence, faded out and back in (Mixxx's ReadAheadManager). */
  private present = 1;
  private lastK = -1;
  private lastChunk: Chunk | undefined;
  readonly marks: Marks;

  constructor(o: { chunks: Chunks; rate: number; outRate: number; fromMs: number; speed: (tMs: number) => number; gain: number }) {
    this.chunks = o.chunks;
    this.speed = o.speed;
    this.gainWanted = o.gain;
    this.gainNow = o.gain;
    this.rate = o.rate;
    this.outRate = o.outRate;
    this.ratio = o.rate / o.outRate;
    this.frames = o.chunks.frames;
    this.at = (o.fromMs * o.rate) / 1000;
    this.fadeStep = 1 / ((o.outRate * FADE_MS) / 1000);
    this.xfade = Math.max(1, Math.floor((o.outRate * XFADE_MS) / 1000));
    this.marks = new Marks(o.rate, o.outRate);
    // The chunks it starts on asked for before it sounds.
    this.chunks.want(ahead(this.at, o.speed(0), 0, o.rate));
  }

  /** Fades out and ends. */
  stop() {
    this.stopping = true;
  }

  /** Its gain from now on (the page's volume, ducking and the track's level), reached over FADE_MS. */
  setGain(g: number) {
    this.gainWanted = g;
  }

  /** Its own time (ms): what its motion's speed is asked at. */
  get ms(): number {
    return (this.written * 1000) / this.outRate;
  }

  private held(j: number, c: number): number | undefined {
    const k = Math.floor(j / CHUNK);
    // Asked again while missing: it may have come since.
    if (k !== this.lastK || !this.lastChunk) {
      this.lastK = k;
      this.lastChunk = this.chunks.get(k);
    }
    const ch = this.lastChunk;
    if (!ch) return undefined;
    const idx = (j - k * CHUNK) * 2 + c;
    return idx < ch.length ? ch[idx] : 0;
  }

  private sample(x: number, c: number): number | undefined {
    if (x < 0 || x >= this.frames - 1) return 0;
    const i = Math.floor(x);
    const f = x - i;
    const y0 = this.held(Math.max(0, i - 1), c);
    const y1 = this.held(i, c);
    const y2 = this.held(i + 1, c);
    const y3 = this.held(Math.min(this.frames - 1, i + 2), c);
    if (y0 === undefined || y1 === undefined || y2 === undefined || y3 === undefined) return undefined;
    return cubic(y0, y1, y2, y3, f);
  }

  /** The output's next frames (from -1 to 1), as many as `left` holds; why it ended once it has, else null. */
  fill(left: Float32Array, right: Float32Array): Ended | null {
    const n = left.length;
    const tMs = (this.written * 1000) / this.outRate;
    const v = this.speed(tMs);
    this.chunks.want(ahead(this.at, v, tMs, this.rate));
    this.marks.add({ frame: this.written, at: this.at, v });
    let missed = false;
    for (let f = 0; f < n; f++) {
      const ms = ((this.written + f) * 1000) / this.outRate;
      const sv = f === 0 ? v : this.speed(ms);
      let reads: number;
      let shape = 1;
      // Faster than heard: in pieces, each read at the top from where the head was as it began.
      if (Math.abs(sv) > TOP) {
        const k = Math.floor(ms / PIECE_MS);
        if (k !== this.piece) {
          this.piece = k;
          this.pieceFrom = this.at;
        }
        const into = ms - k * PIECE_MS;
        reads = this.pieceFrom + (Math.sign(sv) * TOP * into * this.rate) / 1000;
        const a = (into * this.outRate) / 1000;
        const b = ((PIECE_MS - into) * this.outRate) / 1000;
        shape = Math.min(1, a / this.xfade, b / this.xfade);
      } else {
        this.piece = -1;
        reads = this.at;
      }
      if (this.stopping) this.fade -= this.fadeStep;
      else if (this.fade < 1) this.fade = Math.min(1, this.fade + this.fadeStep);
      // Its volume with its speed below a quarter: silence at a stand (the owner, after xwax).
      const bySpeed = Math.min(1, Math.abs(sv) / QUIET_BELOW);
      const l = this.sample(reads, 0);
      const r = this.sample(reads, 1);
      if (l === undefined || r === undefined) {
        missed = true;
        this.present = Math.max(0, this.present - this.fadeStep);
      } else if (this.present < 1) {
        this.present = Math.min(1, this.present + this.fadeStep);
      }
      if (this.gainNow !== this.gainWanted) {
        const d = this.gainWanted - this.gainNow;
        this.gainNow = Math.abs(d) <= this.fadeStep ? this.gainWanted : this.gainNow + Math.sign(d) * this.fadeStep;
      }
      const g = (this.gainNow * Math.max(0, this.fade) * shape * bySpeed * this.present) / 32768;
      left[f] = Math.max(-1, Math.min(1, (l ?? 0) * g));
      right[f] = Math.max(-1, Math.min(1, (r ?? 0) * g));
      this.at += sv * this.ratio;
    }
    if (missed) this.chunks.misses++;
    this.written += n;
    if (this.stopping && this.fade <= 0) return this.reason;
    if (this.at < 0 || this.at >= this.frames) {
      this.reason = this.at < 0 ? 'start' : 'end';
      this.stopping = true;
    }
    return null;
  }
}
