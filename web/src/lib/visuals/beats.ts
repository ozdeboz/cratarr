// A track's beats (decision 0046): every beat's time and how strongly it starts, from the track's analysis on the
// server, which works them out the first time they are asked for (about a second). Read once per track and kept while
// the page is open; the visuals ask for the playing track's and the next one's.
import { PLAYING, askAnalysis } from '../analysisAsks';
import { asksSignIn, toSignIn } from '../reach.svelte';

export type Beats = {
  /** Each beat's time, seconds from the start of the track, in order. */
  at: Float64Array;
  /** How strongly each starts, 0..1 (1: the track's strong beats). */
  strength: Float32Array;
  /** Each beat's typical interval (s): the median of the intervals round it. Beat this! (decision 0049) puts beats
   *  only where it hears a pulse, so a track's beats can stop for a break and start again; the grid before it was
   *  continuous. The typical interval is the tempo through such a gap. */
  typical: Float32Array;
};

/** Where a moment falls among the beats. */
export type Beat = {
  /** The beat at or before it. */
  index: number;
  /** Seconds since that beat. */
  since: number;
  /** Seconds from that beat to the next; the typical interval where the next is a gap away (or there is none). */
  length: number;
  strength: number;
};

/** The server could not work them out then (busy, or the decode took too long): asked again after this long, at most
 *  this many times. */
const AGAIN_MS = 15_000;
const TRIES = 3;

const kept = new Map<string, Beats | null>();
const asking = new Map<string, { at: number; tries: number; busy: boolean }>();

/** A track's beats: undefined while they are asked for, null where it has none (no tempo, or not a file Cratarr keeps). */
export function beatsOf(source: string, id: string, rank = PLAYING): Beats | null | undefined {
  const key = `${source}/${id}`;
  if (kept.has(key)) return kept.get(key);
  const now = performance.now();
  const was = asking.get(key);
  if (was && (was.busy || now - was.at < AGAIN_MS)) return undefined;
  if (was && was.tries >= TRIES) {
    kept.set(key, null);
    return null;
  }
  const tries = (was?.tries ?? 0) + 1;
  asking.set(key, { at: now, tries, busy: true });
  const done = () => asking.set(key, { at: performance.now(), tries, busy: false });
  // `b`: Beat this!'s beats may replace the analysis' own (decision 0049, amended); a browser keeps the older ones,
  // kept a month, under the older address.
  askAnalysis(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/beats?b=2`, rank)
    .then(async (r) => {
      if (asksSignIn(r)) toSignIn();
      if (r.ok) kept.set(key, decode(await r.arrayBuffer()));
      else if (r.status !== 503) kept.set(key, null);
    })
    .then(done, done);
  return undefined;
}

/** Five bytes a beat: its time in milliseconds (u32, little-endian), then its strength (0..=255). None: no tempo. */
export function decode(b: ArrayBuffer): Beats | null {
  const n = Math.floor(b.byteLength / 5);
  if (!n) return null;
  const v = new DataView(b);
  const at = new Float64Array(n);
  const strength = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    at[i] = v.getUint32(i * 5, true) / 1000;
    strength[i] = v.getUint8(i * 5 + 4) / 255;
  }
  return { at, strength, typical: typicalOf(at) };
}

/** Each beat's typical interval: the median of the intervals in the eight beats round it (a gap among them is an
 *  outlier, which the median leaves out); half a second with a single beat. */
function typicalOf(at: Float64Array): Float32Array {
  const n = at.length;
  const out = new Float32Array(n).fill(0.5);
  if (n < 2) return out;
  const gaps = new Float64Array(n - 1);
  for (let i = 0; i < n - 1; i++) gaps[i] = at[i + 1] - at[i];
  for (let i = 0; i < n; i++) {
    const from = Math.max(0, Math.min(i - 4, n - 1 - 8));
    const near = Array.from(gaps.subarray(from, Math.min(n - 1, from + 8))).sort((a, b) => a - b);
    out[i] = near[near.length >> 1];
  }
  return out;
}

/** An interval this many times the typical one is a gap (a break where no pulse is heard), not a slow beat. */
const GAP = 2;

/** The last beat at or before `t` (seconds into the track); -1 before the first. */
function lastBeat(at: Float64Array, t: number): number {
  if (!(t >= at[0])) return -1;
  let lo = 0;
  let hi = at.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (at[mid] <= t) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** Where `t` falls among the beats: null before the first, and from half a beat past where the next would fall when
 *  none comes (after the last, or in a gap, GAP). */
export function beatAt(b: Beats, t: number): Beat | null {
  const i = lastBeat(b.at, t);
  if (i < 0) return null;
  const typical = b.typical[i];
  const next = i + 1 < b.at.length ? b.at[i + 1] - b.at[i] : Infinity;
  const length = next > typical * GAP ? typical : next;
  const since = t - b.at[i];
  if (since > length * 1.5) return null;
  return { index: i, since, length, strength: b.strength[i] };
}

/** The track's tempo at `t` (seconds a beat): the typical interval of the beats there, in a gap and before the first
 *  beat too, so what turns with the tempo keeps it where no beat falls (0049's interim). */
export function tempoAt(b: Beats, t: number): number {
  return b.typical[Math.max(0, lastBeat(b.at, t))];
}
