// A track's sound in three frequency bands (decision 0046, amended): low (kick drum and bass), middle (snare, voices,
// most instruments) and high (hats and cymbals). Each band's loudness every 50 ms and its transients, and the whole
// sound's loudness as heard every 100 ms, from the track's analysis on the server, which works them out the first time
// they are asked for (in the decode that finds its beats). Read once per track and kept while the page is open.
import { asksSignIn, toSignIn } from '../reach.svelte';

export const BANDS = 3;

export type Bands = {
  /** Loudness values a second. */
  rate: number;
  /** Each band's loudness, `rate` a second, 0..1 (1: the band's loudest in the track, 0: 30 dB under it). */
  loudness: Float32Array[];
  /** Each band's transients: their times (seconds, in order) and strengths (0..1). */
  at: Float64Array[];
  strength: Float32Array[];
  /** The loudness as heard (LUFS, EBU R128's momentary), `momentsRate` a second; null from a server before it. */
  moments: Float32Array | null;
  momentsRate: number;
  /** Each band's transients a second, over the whole track, and their mean strength. */
  perSecond: number[];
  meanStrength: number[];
};

/** A band now: how loud it is, how much its last transient still rings (0..1), that transient (its place among the
 *  band's, -1 before the first) and its strength against the band's mean (1: as strong as its transients are on
 *  average), and the band's transients a second over the track. */
export type Band = { loud: number; hit: number; index: number; weight: number; perSecond: number };

/** The server could not work them out then: asked again after this long, at most this many times. */
const AGAIN_MS = 15_000;
const TRIES = 3;

const kept = new Map<string, Bands | null>();
const asking = new Map<string, { at: number; tries: number; busy: boolean }>();

/** A track's bands: undefined while they are asked for, null where it has none (a file Cratarr does not keep, or one
 *  that does not decode whole). */
export function bandsOf(source: string, id: string): Bands | null | undefined {
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
  // `v`: the form read here; a browser keeps an older one under the address without it for a month.
  fetch(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/bands?v=2`)
    .then(async (r) => {
      if (asksSignIn(r)) toSignIn();
      if (r.ok) kept.set(key, decode(await r.arrayBuffer()));
      else if (r.status !== 503) kept.set(key, null);
    })
    .then(done, done);
  return undefined;
}

/** The server's form (`GET /api/track/{source}/{id}/bands`): a format byte (2), the bands (3), band loudness values
 *  a second, loudness-as-heard values a second; the loudness frames (u32, little-endian) and a byte a band in each;
 *  then each band's transients, their number (u32) and each one's milliseconds (u32) and strength (u8); then the
 *  loudness as heard, its number (u32) and a byte each (thirds of a dB from -70 LUFS). Format 1 is the same without the
 *  loudness as heard. Anything else: none. */
function decode(buffer: ArrayBuffer): Bands | null {
  const v = new DataView(buffer);
  const format = buffer.byteLength >= 8 ? v.getUint8(0) : 0;
  if ((format !== 1 && format !== 2) || v.getUint8(1) !== BANDS) return null;
  const rate = v.getUint8(2);
  const momentsRate = format === 2 ? v.getUint8(3) : 0;
  const frames = v.getUint32(4, true);
  let at = 8;
  if (buffer.byteLength < at + frames * BANDS) return null;
  const loudness = Array.from({ length: BANDS }, () => new Float32Array(frames));
  for (let f = 0; f < frames; f++) for (let b = 0; b < BANDS; b++) loudness[b][f] = v.getUint8(at++) / 255;
  const times: Float64Array[] = [];
  const strength: Float32Array[] = [];
  for (let b = 0; b < BANDS; b++) {
    if (buffer.byteLength < at + 4) return null;
    const n = v.getUint32(at, true);
    at += 4;
    if (buffer.byteLength < at + n * 5) return null;
    const t = new Float64Array(n);
    const s = new Float32Array(n);
    for (let i = 0; i < n; i++, at += 5) {
      t[i] = v.getUint32(at, true) / 1000;
      s[i] = v.getUint8(at + 4) / 255;
    }
    times.push(t);
    strength.push(s);
  }
  let moments: Float32Array | null = null;
  if (format === 2) {
    if (buffer.byteLength < at + 4) return null;
    const n = v.getUint32(at, true);
    at += 4;
    if (buffer.byteLength < at + n) return null;
    moments = new Float32Array(n);
    for (let i = 0; i < n; i++) moments[i] = -70 + v.getUint8(at + i) / 3;
  }
  const secs = frames / Math.max(1, rate);
  const mean = (s: Float32Array) => (s.length ? s.reduce((a, b) => a + b, 0) / s.length : 1);
  return {
    rate,
    loudness,
    at: times,
    strength,
    moments,
    momentsRate,
    perSecond: times.map((t) => (secs > 0 ? t.length / secs : 0)),
    meanStrength: strength.map(mean),
  };
}

/** The loudness as heard at `t` seconds (LUFS, between its values); null where the server did not say. */
export function heardAt(b: Bands, t: number): number | null {
  const m = b.moments;
  if (!m || !m.length || !b.momentsRate) return null;
  const x = Math.min(Math.max(t * b.momentsRate, 0), m.length - 1);
  const i = Math.floor(x);
  return m[i] + (m[Math.min(i + 1, m.length - 1)] - m[i]) * (x - i);
}

/** Each band at `t` seconds into the track: its loudness (between its frames) and its last transient's ring, fading
 *  over `rings[band]` seconds (the Lighting settings). */
export function bandsAt(b: Bands, t: number, rings: readonly number[]): Band[] {
  return b.loudness.map((loud, i) => {
    const x = Math.min(Math.max(t * b.rate, 0), loud.length - 1);
    const f = Math.floor(x);
    const level = loud.length ? loud[f] + ((loud[Math.min(f + 1, loud.length - 1)] ?? loud[f]) - loud[f]) * (x - f) : 0;
    const k = lastAt(b.at[i], t);
    const since = k < 0 ? Infinity : t - b.at[i][k];
    const strength = k < 0 ? 0 : b.strength[i][k];
    const hit = since > rings[i] * 5 ? 0 : (0.3 + 0.7 * strength) * Math.exp(-since / rings[i]);
    return { loud: level, hit, index: k, weight: strength / Math.max(0.05, b.meanStrength[i]), perSecond: b.perSecond[i] };
  });
}

/** The last of `at` (in order) at or before `t`; -1 before the first. */
function lastAt(at: Float64Array, t: number): number {
  let lo = 0;
  let hi = at.length - 1;
  if (hi < 0 || !(t >= at[0])) return -1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (at[mid] <= t) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}
