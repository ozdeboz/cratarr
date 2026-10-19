// A track's beats (decision 0046): every beat's time and how strongly it starts, from the track's analysis on the
// server, which works them out the first time they are asked for (about a second). Read once per track and kept while
// the page is open; the visuals ask for the playing track's and the next one's.
import { asksSignIn, toSignIn } from '../reach.svelte';

export type Beats = {
  /** Each beat's time, seconds from the start of the track, in order. */
  at: Float64Array;
  /** How strongly each starts, 0..1 (1: the track's strong beats). */
  strength: Float32Array;
};

/** Where a moment falls among the beats. */
export type Beat = {
  /** The beat at or before it. */
  index: number;
  /** Seconds since that beat. */
  since: number;
  /** Seconds from that beat to the next. */
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
export function beatsOf(source: string, id: string): Beats | null | undefined {
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
  fetch(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/beats?b=2`)
    .then(async (r) => {
      if (asksSignIn(r)) toSignIn();
      if (r.ok) kept.set(key, decode(await r.arrayBuffer()));
      else if (r.status !== 503) kept.set(key, null);
    })
    .then(done, done);
  return undefined;
}

/** Five bytes a beat: its time in milliseconds (u32, little-endian), then its strength (0..=255). None: no tempo. */
function decode(b: ArrayBuffer): Beats | null {
  const n = Math.floor(b.byteLength / 5);
  if (!n) return null;
  const v = new DataView(b);
  const at = new Float64Array(n);
  const strength = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    at[i] = v.getUint32(i * 5, true) / 1000;
    strength[i] = v.getUint8(i * 5 + 4) / 255;
  }
  return { at, strength };
}

/** Where `t` (seconds into the track) falls among its beats; null before the first and more than a beat after the last. */
export function beatAt(b: Beats, t: number): Beat | null {
  const { at } = b;
  // The last beat at or before t.
  let lo = 0;
  let hi = at.length - 1;
  if (!(t >= at[0])) return null;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (at[mid] <= t) lo = mid;
    else hi = mid - 1;
  }
  const length = lo + 1 < at.length ? at[lo + 1] - at[lo] : lo > 0 ? at[lo] - at[lo - 1] : 0.5;
  const since = t - at[lo];
  if (since > length * 1.5) return null;
  return { index: lo, since, length, strength: b.strength[lo] };
}
