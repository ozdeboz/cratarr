// The tape deck's machine as the page sees it (decision 0102, amended 2026-11-04, the owner: "I want a pure tape
// engine"): the rules crate's `deck::next` (server/rules/src/deck.rs, a5's; its table in docs/tape-machine.md), in
// tape.wasm, called here ([machine]) and only carried out. The page keeps no flag of its own about the deck: the deck
// lives in the engine under its handle, and where the head is heard goes in with every event.

/** A way along the tape. */
export type Way = 'forwards' | 'backwards';
/** The direction knob: R forwards, L backwards. Only the user's turn changes it (a new queue sets R). */
export type Knob = 'R' | 'L';
/** At a track's edge while held: stand there, or wind on through the queue (`startStop.edges`). */
export type Edges = 'stop' | 'queue';
/** Repeat: off, this track again, or round the queue. */
export type Repeat = 'off' | 'one' | 'all';

/** The deck's settings, as the machine takes them: each motion winds only where the tape engine is on and that motion
 *  is (off, it is a cut); Start and stop, and whether it brakes skips out; the edges; repeat. */
export type Settings = {
  edges: Edges;
  toEnd: boolean;
  toStart: boolean;
  seekBack: boolean;
  seekFwd: boolean;
  holdBack: boolean;
  holdFwd: boolean;
  startStop: boolean;
  skips: boolean;
  repeat: Repeat;
};
/** Why a motion did not start: its sound came too late, or Cratarr could not be reached for it. */
export type Why = 'late' | 'unreachable';

/** What the deck's tape is doing, as the machine says (for showing it; what to do comes as [Do]s): awaiting is standing
 *  at the queue's end while Autoplay's or Radio's picks come, to play on into them or to stand there. */
export type TapeIs =
  | 'standing'
  | 'playing'
  | 'spinning-up'
  | 'tapping'
  | 'held'
  | 'held-waiting'
  | 'braking-to-one'
  | 'braking-to-stand'
  | 'awaiting-to-play'
  | 'awaiting-standing';

/** Where the head is heard (never a guess): its track in the queue, the queue's length, where in the track (ms), the
 *  track's length where known, and whether the queue grows at its end (Autoplay or Radio asked for more). */
export type Here = { track: number; tracks: number; ms: number; len: number | null; more: boolean };

/** What happens to the deck. */
export type Event =
  | { kind: 'play' }
  | { kind: 'pause' }
  | { kind: 'tap-next' }
  | { kind: 'tap-previous' }
  | { kind: 'hold-start'; way: Way }
  /** Which button was let go (a hold whose setting is off: that button's tap). */
  | { kind: 'hold-end'; way: Way }
  | { kind: 'turn'; knob: Knob }
  /** A seek in the track; `wind`: wound there whatever the settings (another device asked it to). */
  | { kind: 'seek'; to: number; wind: boolean }
  /** The motion in hand finished: a tap landed, a brake reached its speed, a spin-up reached 1×. */
  | { kind: 'done' }
  /** The head reached its track's edge, its way. */
  | { kind: 'edge' }
  /** A motion could not start. */
  | { kind: 'refused'; why: Why }
  /** The sound a waiting hold wants is here. */
  | { kind: 'ready' }
  /** The track played out forwards at 1×. */
  | { kind: 'track-ended' }
  /** A new queue; `play`: it plays at once. */
  | { kind: 'new-queue'; play: boolean }
  /** The queue grew at its end (Autoplay's or Radio's picks came in). */
  | { kind: 'grew' }
  /** A queue entry chosen: a cut there, as a seek is (no skip's wind-down). */
  | { kind: 'jump'; track: number }
  /** At rest at once from anything, the knob untouched: a plain toggle, or a move to another device (every play
   *  effect bypassed). */
  | { kind: 'stop' };

/** A place in a track: its start, its very end (backwards play going on into it), [FROM_END_MS] before its end (where
 *  the knob at L plays a track from, 0.12.4), or ms in. */
export type At = { at: 'start' } | { at: 'end' } | { at: 'from-end' } | { at: 'ms'; ms: number };

/** What to do, in order, from the heard place. */
export type Do =
  /** At 1× that way, the tape moving already: forwards, the track's own player takes it at the heard place. */
  | { do: 'play'; way: Way }
  /** From still up to 1× that way. */
  | { do: 'spin-up'; way: Way }
  /** A tap in this track to `to`, that way: from still or from 1×, landing still or at 1×. */
  | { do: 'tap'; to: At; way: Way; fromStill: boolean; landsStill: boolean }
  /** A hold that way, from still or flown from 1×. */
  | { do: 'hold'; way: Way; fromStill: boolean }
  /** The motion let go: braked to 1× its way, or to a stand. */
  | { do: 'let-go'; toOne: boolean }
  /** A cut to that place, the tape doing as it does. */
  | { do: 'go-to'; track: number; at: At }
  /** On into another track, the motion going on. */
  | { do: 'cross'; track: number; at: At }
  /** At rest at the heard place. */
  | { do: 'stand' }
  /** The deck says why a motion did not start (a hold: that it waits). */
  | { do: 'say'; why: Why };

/** How far before its end the knob at L plays a track from (ms; the rules crate's `tape::FROM_END_MS`). */
export const FROM_END_MS = 1_500;

/** A place in a track as ms in, for a track `lenMs` long (its very end where its length is not known yet: none). */
export function atMs(at: At, lenMs: number | null): number | null {
  switch (at.at) {
    case 'start':
      return 0;
    case 'ms':
      return lenMs === null ? at.ms : Math.min(Math.max(0, at.ms), lenMs);
    case 'end':
      return lenMs;
    case 'from-end':
      return lenMs === null ? null : Math.max(0, lenMs - FROM_END_MS);
  }
}

/** The sign of a way (forwards +1, backwards -1), as the head's motions take it. */
export const signOf = (way: Way): 1 | -1 => (way === 'forwards' ? 1 : -1);

/** tape.wasm's deck calls (the rules crate's wasm.rs, its codes in the comment above `deck_new`). */
export type DeckWasm = {
  deck_new(knob: number): number;
  deck_next(h: number, settings: number, track: number, tracks: number, ms: number, len: number, more: number, event: number, a: number, b: number): number;
  deck_do(i: number, field: number): number;
  deck_knob(h: number): number;
  deck_tape(h: number): number;
  deck_tape_way(h: number): number;
  deck_plays(h: number): number;
  deck_free(h: number): void;
};

const WAY: Way[] = ['forwards', 'backwards'];
const TAPE: TapeIs[] = ['standing', 'playing', 'spinning-up', 'tapping', 'held', 'held-waiting', 'braking-to-one', 'braking-to-stand', 'awaiting-to-play', 'awaiting-standing'];
const wayN = (w: Way) => (w === 'backwards' ? 1 : 0);

/** The settings in the engine's bits. */
export function settingsBits(s: Settings): number {
  const flags: [boolean, number][] = [
    [s.edges === 'queue', 1],
    [s.toEnd, 2],
    [s.toStart, 4],
    [s.seekBack, 8],
    [s.seekFwd, 16],
    [s.holdBack, 32],
    [s.holdFwd, 64],
    [s.startStop, 128],
    [s.skips, 256],
    [s.repeat === 'one', 512],
    [s.repeat === 'all', 1024],
  ];
  return flags.reduce((n, [on, bit]) => (on ? n | bit : n), 0);
}

/** An event in the engine's numbers: its code and its two arguments. */
export function eventCode(e: Event): [number, number, number] {
  switch (e.kind) {
    case 'play':
      return [0, 0, 0];
    case 'pause':
      return [1, 0, 0];
    case 'tap-next':
      return [2, 0, 0];
    case 'tap-previous':
      return [3, 0, 0];
    case 'hold-start':
      return [4, wayN(e.way), 0];
    case 'hold-end':
      return [5, wayN(e.way), 0];
    case 'turn':
      return [6, e.knob === 'L' ? 1 : 0, 0];
    case 'seek':
      return [7, Math.round(e.to), e.wind ? 1 : 0];
    case 'done':
      return [8, 0, 0];
    case 'edge':
      return [9, 0, 0];
    case 'refused':
      return [10, e.why === 'unreachable' ? 1 : 0, 0];
    case 'ready':
      return [11, 0, 0];
    case 'track-ended':
      return [12, 0, 0];
    case 'new-queue':
      return [13, e.play ? 1 : 0, 0];
    case 'grew':
      return [14, 0, 0];
    case 'jump':
      return [15, e.track, 0];
    case 'stop':
      return [16, 0, 0];
  }
}

/** A thing to do from the engine's fields (kind, way or why, place kind, place ms, track, from still or to 1×, lands
 *  still); null for a kind it does not know. */
export function doOf(field: (f: number) => number): Do | null {
  const way = WAY[field(1)] ?? 'forwards';
  const place = (): At => {
    switch (field(2)) {
      case 0:
        return { at: 'start' };
      case 1:
        return { at: 'end' };
      case 2:
        return { at: 'from-end' };
      default:
        return { at: 'ms', ms: field(3) };
    }
  };
  switch (field(0)) {
    case 0:
      return { do: 'play', way };
    case 1:
      return { do: 'spin-up', way };
    case 2:
      return { do: 'tap', to: place(), way, fromStill: field(5) === 1, landsStill: field(6) === 1 };
    case 3:
      return { do: 'hold', way, fromStill: field(5) === 1 };
    case 4:
      return { do: 'let-go', toOne: field(5) === 1 };
    case 5:
      return { do: 'go-to', track: field(4), at: place() };
    case 6:
      return { do: 'cross', track: field(4), at: place() };
    case 7:
      return { do: 'stand' };
    case 8:
      return { do: 'say', why: field(1) === 1 ? 'unreachable' : 'late' };
    default:
      return null;
  }
}

/** A deck in the engine: each event goes in with the deck's settings and where the head is heard, and what to do comes
 *  out, in order; its knob and its tape as the machine has them. */
export type Machine = {
  next(settings: Settings, here: Here, e: Event): Do[];
  knob(): Knob;
  tape(): TapeIs;
  /** The tape's way (standing: the knob's). */
  way(): Way;
  /** Whether the deck ends up playing: what the page shows as playing (a pause's brake: not; a turn's: yes). */
  plays(): boolean;
  free(): void;
};

/** A deck made in the engine `x`, standing, its knob at `knob`; null where the engine has no room for one. */
export function machine(x: DeckWasm, knob: Knob): Machine | null {
  const h = x.deck_new(knob === 'L' ? 1 : 0);
  if (!h) return null;
  return {
    next(settings, here, e) {
      const [code, a, b] = eventCode(e);
      const n = x.deck_next(h, settingsBits(settings), here.track, here.tracks, Math.round(here.ms), here.len ?? -1, here.more ? 1 : 0, code, a, b);
      const out: Do[] = [];
      for (let i = 0; i < n; i++) {
        const d = doOf((f) => x.deck_do(i, f));
        if (d) out.push(d);
      }
      return out;
    },
    knob: () => (x.deck_knob(h) === 1 ? 'L' : 'R'),
    tape: () => TAPE[x.deck_tape(h)] ?? 'standing',
    way: () => WAY[x.deck_tape_way(h)] ?? 'forwards',
    plays: () => x.deck_plays(h) === 1,
    free: () => x.deck_free(h),
  };
}
