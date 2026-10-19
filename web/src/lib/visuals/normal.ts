// The music's signals on one scale (decision 0056, step 5): 1 = typical, so a cable or a modulator has room above and
// below it to swing, and "above 1" means what MilkDrop's presets mean by it. What every cable and modulator reads.
//
// - **This track** (the default): against the signal's typical value in this track, the whole song's median, so the
//   drop stays the biggest moment. Worked out in the page from the server's signals it holds whole (`typicalOf`): each
//   series' median where it sounds (its frames above 0, so a signal that is mostly still, a swell or the harmonic
//   change, is 1 at its typical size when it comes), a band's hits and the sections by their median strength, the
//   beats' pulses by their median strength (at the moment of the beat), the tempo by the beats' median interval. Where
//   the server's are not in yet (the sound heard as it plays, live.ts) and for what it has no series of (each band's
//   Envelope), from what has been heard of the track so far: the same medians, so live and server give one scale.
// - **Lately**: against its own mean over the last few seconds (`LATELY_S`; MilkDrop's bass, mid and treble are this).
// - **Absolute**: as today, each 0..1. The loudness (which Intensity follows) is this unless a row asks for another;
//   the key (a place on the circle of fifths) is only ever this.
// - Each **fast** (the frame's) or **slow** (followed: up over `SLOW_UP_S`, down over `SLOW_DOWN_S`, MilkDrop's `_att`).
//
// Pure: no DOM, no Svelte; the visuals' loop calls `signalsFor` once a frame with the frame's `Music`.

import type { Beats } from './beats';
import type { SignalRef } from './patch';
import { B, BAND_SIGNALS, T, TRACK_SIGNALS } from './routes';
import { ID } from './signal-ids';
import type { Signals } from './signals';

/** Bands the signals have (signals.ts, live.ts). */
const BANDS = 6;
/** Lately: the mean over about this long (s). */
export const LATELY_S = 4;
/** The slow form rises over this long and falls over this long (s). */
export const SLOW_UP_S = 0.12;
export const SLOW_DOWN_S = 0.5;
/** No value goes above this many times typical: one wild frame does not throw a knob to its end. */
export const MOST = 8;
/** A typical value is at least this (four steps of the server's bytes): a series that is mostly noise does not make
 *  every blip above it read as many times typical. */
export const FLOOR = 4 / 255;
/** Below this a mean is none (the signal is not there). */
const NONE = 1e-3;
/** What has been heard of a signal takes over from its value as it is over this much of it: seconds of a sound, or
 *  moments (hits, beats) of a moment's; before that the two are crossed, so the scale never jumps. */
export const WARM_S = 5;
export const WARM_MOMENTS = 8;
/** The histogram of what has been heard: a step a byte of the server's (values of 1 and above in the top one). */
const STEPS = 256;

/** A track's typical values, `NaN` where it has none: each track signal (`TRACK_SIGNALS` order) and each band's
 *  (`BAND_SIGNALS` order). The loudness has none here: Intensity is the loudness at the player's gain, so its typical
 *  is what has been heard of it. */
export type Typical = { track: Float32Array; bands: Float32Array[] };

/** What the frame's signals are, as music.svelte.ts gives them (its `Music`). */
export type MusicNow = {
  key: string | null;
  /** Whether it plays: a paused frame is not heard, nor followed. */
  playing: boolean;
  track: Float32Array;
  bands: Float32Array[] | null;
  /** The track's typical values from the server's signals and beats, where they are in. */
  typical: Typical | null;
};

/** Signals that are moments (a pulse at each, fading): typical is their value at the moment. */
const MOMENT_TRACK = new Set<number>([T.beat, T.downbeat, T.section, T.track]);
const MOMENT_BAND = new Set<number>([B.hits, B.percussiveHits]);
/** Only ever absolute. */
const ABSOLUTE_ONLY = new Set<number>([T.key]);
/** Absolute unless a row says otherwise. */
const ABSOLUTE_BY_DEFAULT = new Set<number>([T.loudness, T.key]);

const median = (values: number[]): number => {
  if (!values.length) return Number.NaN;
  const v = values.slice().sort((a, b) => a - b);
  const m = v.length >> 1;
  return v.length % 2 ? v[m] : (v[m - 1] + v[m]) / 2;
};

/** A series' channel `c` where it sounds (its frames above 0), each 0..1 as `seriesAt` reads them. */
function sounding(s: Signals, id: number, c: number): number[] {
  const x = s.series.get(id);
  if (!x || c >= x.channels) return [];
  const out: number[] = [];
  for (let i = c; i < x.data.length; i += x.channels) if (x.data[i] > 0) out.push(x.data[i] / 255);
  return out;
}

const cache = new WeakMap<object, Map<object | null, Typical>>();

/** A track's typical values, from the server's signals (null: not in) and its beats (null: none known). Kept for each
 *  pair, so it is worked out once a track. */
export function typicalOf(signals: Signals | null, beats: Beats | null): Typical {
  const holder = (signals ?? beats ?? cache) as object;
  let byBeats = cache.get(holder);
  if (!byBeats) cache.set(holder, (byBeats = new Map()));
  const had = byBeats.get(beats);
  if (had) return had;
  const t: Typical = {
    track: new Float32Array(TRACK_SIGNALS.length).fill(Number.NaN),
    bands: Array.from({ length: BANDS }, () => new Float32Array(BAND_SIGNALS.length).fill(Number.NaN)),
  };
  t.track[T.track] = 1;
  if (beats && beats.at.length > 1) {
    const strengths = Array.from(beats.strength);
    t.track[T.beat] = 0.35 + 0.65 * median(strengths);
    const intervals: number[] = [];
    for (let i = 1; i < beats.at.length; i++) intervals.push(beats.at[i] - beats.at[i - 1]);
    const interval = median(intervals);
    if (interval > 0) t.track[T.tempo] = 1 / interval / 2;
    // The downbeats: the beats that start their bar, as surely as the server is of where bars start (music.svelte.ts).
    const bar = signals?.series.get(ID.bar);
    if (bar && bar.channels === 1) {
      const surely = Math.min(1, (2 * bar.scale) / 255);
      const down = strengths.filter((_, i) => i < bar.data.length && bar.data[i] === 0);
      if (down.length && surely > 0) t.track[T.downbeat] = (0.35 + 0.65 * median(down)) * surely;
    }
  }
  if (signals) {
    const marks = signals.marks.get(ID.boundaries);
    if (marks?.value.length) t.track[T.section] = median(Array.from(marks.value));
    for (const [signal, id] of [
      [T.harmony, ID.harmonicChange],
      [T.centroid, ID.centroid],
      [T.swell, ID.swell],
      [T.buildUp, ID.buildUp],
    ] as const)
      t.track[signal] = median(sounding(signals, id, 0));
    for (let b = 0; b < BANDS; b++) {
      for (const [signal, id] of [
        [B.level, ID.level],
        [B.flux, ID.flux],
        [B.percussive, ID.percussive],
        [B.harmonic, ID.harmonic],
        [B.crest, ID.crest],
        [B.width, ID.width],
        [B.bandSwell, ID.bandSwell],
      ] as const)
        t.bands[b][signal] = median(sounding(signals, id, b));
      for (const [signal, id] of [
        [B.hits, ID.hits],
        [B.percussiveHits, ID.percussiveHits],
      ] as const) {
        const hits = signals.hits.get(id)?.[b];
        if (hits?.value.length) t.bands[b][signal] = median(Array.from(hits.value));
      }
    }
  }
  byBeats.set(beats, t);
  return t;
}

/** A signal followed: its slow form, its lately mean, what has been heard of it (a histogram, by time for a sound and
 *  by moments for a moment's), its median as last worked out (in frame `medianFrame`), and its last two values (a
 *  moment's top is heard as it starts to fall). */
type Followed = { slow: number; lately: number; heard: Float32Array; weight: number; median: number; medianFrame: number; last: number; before: number };
const followed = (): Followed => ({ slow: 0, lately: 0, heard: new Float32Array(STEPS), weight: 0, median: Number.NaN, medianFrame: -1, last: 0, before: 0 });

/** What `signalsFor` keeps from frame to frame. */
export type NormalState = { key: string | null; at: number; frame: number; track: Followed[]; bands: Followed[][] };

export function newNormalState(): NormalState {
  return {
    key: null,
    at: -1,
    frame: 0,
    track: TRACK_SIGNALS.map(followed),
    bands: Array.from({ length: BANDS }, () => BAND_SIGNALS.map(followed)),
  };
}

/** The median of what has been heard (NaN before anything), worked out once a frame. */
function heardMedian(f: Followed, frame: number): number {
  if (f.medianFrame === frame) return f.median;
  f.medianFrame = frame;
  f.median = Number.NaN;
  if (!(f.weight > 0)) return f.median;
  let seen = 0;
  for (let i = 0; i < STEPS; i++) {
    seen += f.heard[i];
    if (seen * 2 >= f.weight) return (f.median = (i + 0.5) / STEPS);
  }
  return f.median;
}

function hear(f: Followed, v: number, weight: number) {
  f.heard[Math.min(STEPS - 1, Math.max(0, Math.floor(v * STEPS)))] += weight;
  f.weight += weight;
}

/** Follows one signal a frame on (`dt` s; −1: the first frame, or a new track), noting what is heard of it: every value
 *  above 0 for as long as it lasts, or for a moment its top. The lately mean starts at the first sound after silence,
 *  so a return from a break is not many times what came before. */
function follow(f: Followed, v: number, dt: number, moment: boolean) {
  if (!Number.isFinite(v) || v < 0) v = 0;
  if (dt < 0) {
    f.slow = v;
    f.lately = v;
    f.last = v;
    f.before = v;
    return;
  }
  f.slow += (v - f.slow) * (1 - Math.exp(-dt / (v > f.slow ? SLOW_UP_S : SLOW_DOWN_S)));
  if (f.lately < NONE && v > 0) f.lately = v;
  else f.lately += (v - f.lately) * (1 - Math.exp(-dt / LATELY_S));
  if (moment) {
    // Its top: the last frame's, once it falls.
    if (f.last > 0 && v < f.last && f.last >= f.before) hear(f, f.last, 1);
  } else if (v > 0) hear(f, v, dt);
  f.before = f.last;
  f.last = v;
}

/** The frame's signals, 1 = typical (see above), for the cables and the modulators: follows each signal a frame on, and
 *  gives a reader. `at`: the frame's time (ms). A band signal without a band reads the bands' mean. */
export function signalsFor(st: NormalState, music: MusicNow, at: number): (ref: SignalRef) => number {
  const fresh = st.key !== music.key || st.at < 0 || at < st.at;
  if (st.key !== music.key) {
    const clean = newNormalState();
    st.track = clean.track;
    st.bands = clean.bands;
    st.key = music.key;
  }
  const dt = fresh ? -1 : Math.min(0.25, (at - st.at) / 1000);
  st.at = at;
  st.frame++;
  // Paused: nothing is heard, nothing moves on (the first frame still starts each from where it is).
  if (music.playing || dt < 0) {
    for (let i = 0; i < TRACK_SIGNALS.length; i++) follow(st.track[i], music.track[i], dt, MOMENT_TRACK.has(i));
    for (let b = 0; b < BANDS; b++) {
      const v = music.bands?.[b];
      for (let i = 0; i < BAND_SIGNALS.length; i++) follow(st.bands[b][i], v ? v[i] : 0, dt, MOMENT_BAND.has(i));
    }
  }
  const typical = music.typical;

  const one = (signal: number, band: number | null, ref: SignalRef): number => {
    const f = band === null ? st.track[signal] : st.bands[band][signal];
    const moment = band === null ? MOMENT_TRACK.has(signal) : MOMENT_BAND.has(signal);
    const now = band === null ? music.track[signal] : (music.bands?.[band]?.[signal] ?? 0);
    const given = ref.value;
    const raw = given !== undefined ? (Number.isFinite(given) && given > 0 ? given : 0) : ref.speed === 'slow' ? f.slow : Number.isFinite(now) && now > 0 ? now : 0;
    const reference = ABSOLUTE_ONLY.has(signal) && band === null ? 'absolute' : (ref.reference ?? (band === null && ABSOLUTE_BY_DEFAULT.has(signal) ? 'absolute' : 'track'));
    if (reference === 'absolute') return raw;
    // This track's typical: the server's, else what has been heard, crossed with the value as it is while little has.
    const known = band === null ? (typical?.track[signal] ?? Number.NaN) : (typical?.bands[band]?.[signal] ?? Number.NaN);
    const heard = known > 0 ? Number.NaN : heardMedian(f, st.frame);
    const typ = known > 0 ? known : heard;
    if (reference === 'lately') {
      // Never against less than a quarter of typical, where that is known: a quiet stretch is not many times itself.
      const under = Math.max(NONE, typ > 0 ? 0.25 * Math.max(FLOOR, typ) : 0);
      return Math.min(MOST, raw / Math.max(f.lately, under));
    }
    if (!(typ > 0)) return raw;
    const ratio = Math.min(MOST, raw / Math.max(FLOOR, typ));
    const warm = known > 0 ? 1 : Math.min(1, f.weight / (moment ? WARM_MOMENTS : WARM_S));
    return raw + (ratio - raw) * warm;
  };

  return (ref: SignalRef) => {
    const t = TRACK_INDEX.get(ref.signal);
    if (t !== undefined) return one(t, null, ref);
    const b = BAND_INDEX.get(ref.signal);
    if (b === undefined) return 0;
    if (ref.band !== undefined) return ref.band >= 0 && ref.band < BANDS ? one(b, ref.band | 0, ref) : 0;
    let sum = 0;
    for (let band = 0; band < BANDS; band++) sum += one(b, band, ref);
    return sum / BANDS;
  };
}

const TRACK_INDEX = new Map<string, number>(TRACK_SIGNALS.map((s, i) => [s, i]));
const BAND_INDEX = new Map<string, number>(BAND_SIGNALS.map((s, i) => [s, i]));