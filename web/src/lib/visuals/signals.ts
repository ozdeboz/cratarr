// A track's signals (decision 0046, amended; format 3, `GET /api/track/{source}/{id}/bands`): what the server hears in it,
// in one decode the first time they are asked for or ahead of a queue. Six bands' level, flux, hits, percussive and
// harmonic parts, crest and width; the whole track's loudness as heard, spectral centroid, chroma, harmonic change,
// section novelty and boundaries, bar position, swells and build-up. Read once per track and kept while the page is
// open; the visuals ask for the playing track's and the next one's.
import { PLAYING, askAnalysis } from '../analysisAsks';
import { asksSignIn, toSignIn } from '../reach.svelte';

export { ID } from './signal-ids';
import { ID } from './signal-ids';

const KIND = { series: 0, hits: 1, marks: 2, table: 3 } as const;

/** Series whose frame i is the moment i/rate (the loudness, each window by its middle, and what is worked out from it;
 *  novelty two a second where a track has no beats). Every other series' frame i holds what fell in [i/rate,
 *  (i+1)/rate) (analysis/signals.rs, `bucket`): its moment is that frame's middle. */
const AT_MOMENTS = new Set<number>([ID.loudness, ID.novelty, ID.swell, ID.buildUp]);

/** A series: `rate` values a second (0: a value a beat), `channels` bytes a frame. */
export type Series = { rate: number; channels: number; scale: number; data: Uint8Array };
/** Times (seconds, in order) with a byte each, 0..1. */
export type Events = { at: Float64Array; value: Float32Array };

export type Signals = {
  /** The bands' edges (Hz; 0 open), low to high. */
  edges: [number, number][];
  series: Map<number, Series>;
  /** Hits sections, a list a band. */
  hits: Map<number, Events[]>;
  marks: Map<number, Events>;
  /** Each band's hits a second over the track, and their mean strength (for steps as large on average as steady
   *  motion would be). */
  perSecond: number[];
  meanStrength: number[];
};

/** The server could not work them out then: asked again after this long, at most this many times. */
const AGAIN_MS = 15_000;
const TRIES = 3;

const kept = new Map<string, Signals | null>();
const asking = new Map<string, { at: number; tries: number; busy: boolean }>();

/** A track's signals: undefined while they are asked for (or worked out), null where it has none (a file Cratarr does
 *  not keep, or one that does not decode whole). */
export function signalsOf(source: string, id: string, rank = PLAYING): Signals | null | undefined {
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
  // `v`: the form read here; `b`: Beat this!'s beats, which the signals follow (decision 0049, amended). A browser
  // keeps older ones under another address.
  askAnalysis(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/bands?v=3&b=2`, rank)
    .then(async (r) => {
      if (asksSignIn(r)) toSignIn();
      if (r.ok) kept.set(key, decode(await r.arrayBuffer()));
      else if (r.status !== 503) kept.set(key, null);
    })
    .then(done, done);
  return undefined;
}

/** The server's form: a format byte (3), the number of sections, then each section's header (id, kind, rate in tenths
 *  of Hz, channels, count, scale: 10 bytes, little-endian) and its bytes. Anything else: none. */
export function decode(buffer: ArrayBuffer): Signals | null {
  const v = new DataView(buffer);
  const n = buffer.byteLength;
  if (n < 2 || v.getUint8(0) !== 3) return null;
  const out: Signals = { edges: [], series: new Map(), hits: new Map(), marks: new Map(), perSecond: [], meanStrength: [] };
  let at = 2;
  const events = (count: number): Events | null => {
    if (n < at + count * 5) return null;
    const e: Events = { at: new Float64Array(count), value: new Float32Array(count) };
    for (let i = 0; i < count; i++, at += 5) {
      e.at[i] = v.getUint32(at, true) / 1000;
      e.value[i] = v.getUint8(at + 4) / 255;
    }
    return e;
  };
  for (let s = v.getUint8(1); s > 0; s--) {
    if (n < at + 10) return null;
    const id = v.getUint8(at);
    const kind = v.getUint8(at + 1);
    const rate = v.getUint16(at + 2, true) / 10;
    const channels = v.getUint8(at + 4);
    const count = v.getUint32(at + 5, true);
    const scale = v.getUint8(at + 9);
    at += 10;
    if (kind === KIND.series) {
      const len = count * channels;
      if (n < at + len) return null;
      out.series.set(id, { rate, channels, scale, data: new Uint8Array(buffer, at, len) });
      at += len;
    } else if (kind === KIND.hits) {
      const lists: Events[] = [];
      for (let c = 0; c < channels; c++) {
        if (n < at + 4) return null;
        const k = v.getUint32(at, true);
        at += 4;
        const e = events(k);
        if (!e) return null;
        lists.push(e);
      }
      out.hits.set(id, lists);
    } else if (kind === KIND.marks) {
      const e = events(count);
      if (!e) return null;
      out.marks.set(id, e);
    } else if (kind === KIND.table) {
      if (n < at + count * 4) return null;
      for (let i = 0; i < count; i++, at += 4) out.edges.push([v.getUint16(at, true), v.getUint16(at + 2, true)]);
    } else return null;
  }
  if (at !== n) return null;
  // The track's length, from its loudness (10 a second): each band's hits a second over it.
  const loud = out.series.get(ID.loudness);
  const secs = loud && loud.rate > 0 ? loud.data.length / loud.channels / loud.rate : 0;
  const hits = out.hits.get(ID.hits) ?? [];
  out.perSecond = hits.map((h) => (secs > 0 ? h.at.length / secs : 0));
  out.meanStrength = hits.map((h) => (h.value.length ? h.value.reduce((a, b) => a + b, 0) / h.value.length : 1));
  return out;
}

/** A series' channels at `t` seconds (each 0..1, between its frames), or at beat `beat` for a series of a value a beat;
 *  null where the track has none. */
export function seriesAt(s: Signals, id: number, t: number, beat: number | null = null): Float32Array | null {
  const x = s.series.get(id);
  if (!x || !x.channels) return null;
  const frames = x.data.length / x.channels;
  if (!frames) return null;
  const out = new Float32Array(x.channels);
  if (x.rate === 0) {
    if (beat === null) return null;
    const f = Math.min(Math.max(beat, 0), frames - 1);
    for (let c = 0; c < x.channels; c++) out[c] = x.data[f * x.channels + c] / 255;
    return out;
  }
  const pos = Math.min(Math.max(t * x.rate - (AT_MOMENTS.has(id) ? 0 : 0.5), 0), frames - 1);
  const f = Math.floor(pos);
  const g = Math.min(f + 1, frames - 1);
  for (let c = 0; c < x.channels; c++) {
    const a = x.data[f * x.channels + c];
    out[c] = (a + (x.data[g * x.channels + c] - a) * (pos - f)) / 255;
  }
  return out;
}

/** The loudness as heard at `t` (LUFS), or null. */
export function lufsAt(s: Signals, t: number): number | null {
  const v = seriesAt(s, ID.loudness, t);
  return v ? -70 + (v[0] * 255) / 3 : null;
}

/** The last of `e` at or before `t`: its place (-1 before the first), seconds since, and its value. */
export function lastOf(e: Events | undefined, t: number): { index: number; since: number; value: number } {
  if (!e || !e.at.length || !(t >= e.at[0])) return { index: -1, since: Infinity, value: 0 };
  let lo = 0;
  let hi = e.at.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (e.at[mid] <= t) lo = mid;
    else hi = mid - 1;
  }
  return { index: lo, since: t - e.at[lo], value: e.value[lo] };
}
