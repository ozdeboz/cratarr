// A hold's waveform as a strip (lib/Waveform; the owner, 2026-10-29: a held wind went in strides, the tablet redrawing
// every bar of four layers each frame): drawn once for three times the stretch shown, then slid and scaled by the
// compositor as the stretch moves, drawn anew once the stretch leaves it or the zoom has moved on.
// The same number of bars across the waveform at every moment, through the zoom and the slide (0.12, the owner: "the
// waveform is no longer pure through zoom!!! Not the same number of sections in the viewport permanently"; 0.11's
// power-of-two levels showed between n and 2n, two of them blended as the zoom went):
// - Places are in a frame that stands for the whole hold (s from a track of it, lib/SeekWave), each track its own
//   stretch of it with its own bins, against its own loudest: one gain per track, from the whole track.
// - The bars stand on one grid, `span` apart from an origin `o`: a span of the stretch shown over n. At one zoom the
//   grid stands in the music (the slide moves the bars, never their heights); as the zoom moves, the grid is scaled
//   about the middle of what shows (bars widen and narrow from there, none rushing past), and, nearing the track's own
//   view, its grid comes into line with that view's, so the hold hands over to the plain waveform without a step.
// - A bar's height is the music's envelope at its middle, at a scale matched to its span: the loudest of a window as
//   wide as the bar, sliding, and taken between the window sizes either side (a pyramid of maxima, interpolated in time
//   and in size). It moves on continuously with the zoom; a single loud moment fades in and out as a bar's edge
//   passes, never popping.

/** How much wider than the stretch shown a strip is drawn. */
export const STRIP_SPAN = 3;
/** How far the bars shown may drift from n (a share of a bar across the waveform) before a strip is drawn anew. */
export const SPAN_SLACK = 0.25;

/** A track of a hold's view: where it starts in the frame and how long it is (the frame's units), its bins (null: not
 *  known yet), and what it is (its colour's key). */
export type StripTrack = { key: string; start: number; d: number; bins: Uint8Array | null };

/** A track's bins against its own loudest (0..1), with a sparse table of maxima: row k the loudest of 2^k bins from
 *  each. Kept by the bins themselves (a track's bins are made once, lib/waveforms). */
type Table = { st: Float32Array[] };
const tables = new WeakMap<Uint8Array, Table>();
function tableOf(bins: Uint8Array): Table {
  const kept = tables.get(bins);
  if (kept) return kept;
  const L = bins.length;
  let top = 1;
  for (let j = 0; j < L; j++) top = Math.max(top, bins[j]);
  const v = new Float32Array(L);
  for (let j = 0; j < L; j++) v[j] = bins[j] / top;
  const st = [v];
  for (let k = 1; 1 << k <= L; k++) {
    const prev = st[k - 1];
    const half = 1 << (k - 1);
    const row = new Float32Array(L - (1 << k) + 1);
    for (let j = 0; j < row.length; j++) row[j] = Math.max(prev[j], prev[j + half]);
    st.push(row);
  }
  const t = { st };
  tables.set(bins, t);
  return t;
}

/** Row k's window from `p` (bins), between its samples half a window apart: a loud bin comes in as a window's edge
 *  nears it, over half a window, not at once. */
function rowAt(t: Table, k: number, p: number): number {
  const row = t.st[k];
  const h = Math.max(1, 2 ** (k - 1));
  const q = Math.min(row.length - 1, Math.max(0, p)) / h;
  const i = Math.floor(q);
  const a = row[Math.min(row.length - 1, i * h)];
  const b = row[Math.min(row.length - 1, (i + 1) * h)];
  return a + (b - a) * (q - i);
}

/** The envelope at `c` (bins) for a bar `m` bins wide: the sliding loudest of a window that wide, between the window
 *  sizes either side of it. */
function envelope(t: Table, c: number, m: number): number {
  const top = t.st.length - 1;
  const x = Math.min(top, Math.max(0, Math.log2(Math.max(1, m))));
  const k0 = Math.floor(x);
  const k1 = Math.min(top, k0 + 1);
  const a = rowAt(t, k0, c - 2 ** k0 / 2);
  if (k1 === k0) return a;
  return a + (rowAt(t, k1, c - 2 ** k1 / 2) - a) * (x - k0);
}

/**
 * A bar's height from `a` to `b` of the frame (0..1, each track against its own loudest; 0 where none): each track's
 * envelope at the bar's middle at the bar's span, by as much of the bar as the track covers (a bar across a boundary
 * takes the next track in as it reaches into it).
 */
export function loudness(tracks: readonly StripTrack[], a: number, b: number): number {
  const span = b - a;
  const mid = (a + b) / 2;
  let m = 0;
  for (const t of tracks) {
    if (!t.bins || !t.bins.length || t.d <= 0) continue;
    const over = Math.min(b, t.start + t.d) - Math.max(a, t.start);
    if (over <= 0) continue;
    const per = t.bins.length / t.d;
    const c = Math.min(t.d, Math.max(0, mid - t.start)) * per;
    m = Math.max(m, Math.min(1, over / (span / 2)) * envelope(tableOf(t.bins), c, span * per));
  }
  return m;
}

/** A strip as drawn: its grid (`span` apart from `o`), its first bar and how many, and the stretch of the frame they
 *  cover. */
export type GridStrip = { span: number; o: number; g0: number; count: number; e0: number; e1: number };

/** The strip of the grid `span` from `o` to draw for the stretch `from`..`to`: STRIP_SPAN times it, the stretch in
 *  its middle. */
export function gridCover(span: number, o: number, from: number, to: number): GridStrip {
  const w = to - from;
  const g0 = Math.floor((from - ((STRIP_SPAN - 1) / 2) * w - o) / span);
  const g1 = Math.ceil((to + ((STRIP_SPAN - 1) / 2) * w - o) / span);
  return { span, o, g0, count: Math.max(1, g1 - g0), e0: o + g0 * span, e1: o + g1 * span };
}

/** Whether a strip as drawn still covers the stretch. */
export const covers = (s: GridStrip, from: number, to: number) => from >= s.e0 && to <= s.e1;

/** A strip's bars, 0..1 each (a function of the bar's place and span alone). */
export function gridBars(tracks: readonly StripTrack[], s: GridStrip): number[] {
  return Array.from({ length: s.count }, (_, i) => loudness(tracks, s.o + (s.g0 + i) * s.span, s.o + (s.g0 + i + 1) * s.span));
}

/** A whole track's bars in `n` (the plain waveform): the same rule as a hold's, on the grid its own view has, so a hold
 *  that ends on the track's own view hands over without a step. */
export function trackBars(bins: Uint8Array, n: number): number[] {
  const tracks = [{ key: '', start: 0, d: 1, bins }];
  return gridBars(tracks, { span: 1 / n, o: 0, g0: 0, count: n, e0: 0, e1: 1 });
}

/** Which track each bar's middle is in (its index in `tracks`; -1: none), for its colour. */
export function barTracks(tracks: readonly StripTrack[], s: GridStrip): number[] {
  return Array.from({ length: s.count }, (_, i) => {
    const x = s.o + (s.g0 + i + 0.5) * s.span;
    return tracks.findIndex((t) => x >= t.start && x < t.start + t.d);
  });
}

/** A strip's bars track by track (lib/Waveform's played layer, one path each): each run of bars with the track they are
 *  in (`owner`, by its index in `tracks`; -1: none, before the queue or past it) and an `id` that is that track's alone,
 *  its key and its start in the hold's frame, whatever else the strip covers. A path drawn for one track is never drawn
 *  for another (0.12.4, the owner: "a short pulse of that previous track's waveform": the paths went by their order in
 *  the strip, so a strip drawn anew with a track more or less before the playing one handed one track's path to
 *  another, and its fill eased from the one colour to the other over a Start and stop ramp, a hold back's let go
 *  spinning the track up). */
export function playedRuns(owners: readonly number[], tracks: readonly StripTrack[]): { owner: number; id: string; from: number; to: number }[] {
  const out: { owner: number; id: string; from: number; to: number }[] = [];
  owners.forEach((o, i) => {
    const t = tracks[o];
    const id = t ? `${t.key}@${t.start}` : out.some((r) => r.owner >= 0) ? 'past' : 'before';
    if (out.at(-1)?.id === id) out.at(-1)!.to = i + 1;
    else out.push({ owner: o, id, from: i, to: i + 1 });
  });
  return out;
}

/** Where a strip stands for the stretch on a waveform `width` px wide, drawn as wide as the waveform: scaled by `k` from
 *  its left, then slid by `x` px, so that `from` is at the waveform's left and `to` at its right. */
export function placeOf(s: GridStrip, from: number, to: number, width: number): { x: number; k: number } {
  const w = Math.max(1e-9, to - from);
  return { x: -((from - s.e0) / w) * width, k: (s.e1 - s.e0) / w };
}

/** Each track's bins by what they are, for telling them apart. */
const binIds = new WeakMap<Uint8Array, number>();
let binCount = 0;
const binsId = (b: Uint8Array | null) => {
  if (!b) return 0;
  let id = binIds.get(b);
  if (id === undefined) binIds.set(b, (id = ++binCount));
  return id;
};
/** What a strip was drawn from: the tracks it covers, each with its bins (a track's bins come later: drawn anew). */
export function stripSource(tracks: readonly StripTrack[], s: GridStrip): string {
  return tracks
    .filter((t) => t.start < s.e1 && t.start + t.d > s.e0)
    .map((t) => `${t.key}@${Math.round(t.start * 1e6)}+${Math.round(t.d * 1e6)}:${binsId(t.bins)}`)
    .join(',');
}

/** The grid's origin as the zoom moves from `had` to `span`: scaled about the middle of what shows (`mid`), and as the
 *  span nears the track's own view's (`home`), drawn into line with that view's grid (its start on a bar's edge). */
export function originFor(had: { span: number; o: number }, span: number, mid: number, home: { from: number; to: number } | null, n: number): number {
  const o = mid - (mid - had.o) * (span / had.span);
  if (!home) return o;
  const own = (home.to - home.from) / Math.max(1, n);
  // How near the track's own view the zoom is: nothing below half its span, all of it there.
  const u = Math.min(1, Math.max(0, (span / own - 0.5) / 0.5));
  if (u <= 0) return o;
  const lined = home.from + Math.round((o - home.from) / span) * span;
  return o + (lined - o) * u * u * (3 - 2 * u);
}

/**
 * The strip to show for the stretch `from`..`to` in `n` bars, from the one shown (`had`): it, where it still covers the
 * stretch, was drawn from the same tracks and its bars across the stretch are still n (within SPAN_SLACK); its grid
 * drawn anew for the stretch where only the coverage or the tracks changed (the same bars where it had them); else a
 * grid of the stretch's own span. `home`: the track's own view, whose grid the zoom comes into line with.
 */
export function planStrip<S extends GridStrip & { src: string }>(
  had: S | null,
  tracks: readonly StripTrack[],
  from: number,
  to: number,
  n: number,
  home: { from: number; to: number } | null,
): S | (GridStrip & { src: string }) {
  const span = (to - from) / Math.max(1, n);
  const make = (sp: number, o: number) => {
    const c = gridCover(sp, o, from, to);
    return { ...c, src: stripSource(tracks, c) };
  };
  if (!had) return make(span, home ? home.from : 0);
  const slack = SPAN_SLACK / Math.max(1, n);
  // At the track's own view: its grid exactly (the plain waveform's, lib/Waveform's trackBars), whatever the zoom left.
  if (home) {
    const own = (home.to - home.from) / Math.max(1, n);
    if (Math.abs(own / span - 1) <= slack) {
      const lined = home.from + Math.round((had.o - home.from) / own) * own;
      if (had.span === own && had.o === lined && covers(had, from, to) && had.src === stripSource(tracks, had)) return had;
      return make(own, lined);
    }
  }
  const near = Math.abs(had.span / span - 1) <= slack;
  if (near) {
    if (covers(had, from, to) && had.src === stripSource(tracks, had)) return had;
    return make(had.span, had.o);
  }
  return make(span, originFor(had, span, (from + to) / 2, home, n));
}
