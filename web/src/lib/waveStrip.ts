// A hold's waveform as strips (lib/Waveform; the owner, 2026-10-29: a held wind went in strides, the tablet redrawing
// every bar of four layers each frame): each drawn once for three times the stretch shown, then slid and scaled by the
// compositor as the stretch moves, drawn anew only once the stretch leaves it.
// The same moment of music always draws the same bar (0.11.1, the owner: "the waveforms are entirely unstable, and
// possibly the auto gain fill is also rapidly changing … we need everything a whole lot more stable"; 0.11.0 drew the
// queue's tracks as one source made again as the window took in another track, its bars on a grid of that source's
// share, a step ladder 1.25x apart cross-fading at each step, and the zoom's contrast changing with it):
// - Places are in a frame that stands for the whole hold (s from a track of it, lib/SeekWave), each track its own
//   stretch of it with its own bins, against its own loudest: one gain per track, from the whole track.
// - Bars stand on power-of-two grids of that frame (a pyramid of levels, `unit` · 2^L apart), each bar the loudest of
//   the music under it: a function of its place alone. Drawn anew, a level's strip has the same bars where it had them.
// - The zoom between two levels shows both, the finer fading out as the coarser fades in by the zoom itself, so a
//   change of zoom never swaps one set of bars for another at a moment (no cross-fade in time).

/** How much wider than the stretch shown a strip is drawn. */
export const STRIP_SPAN = 3;

/** A track of a hold's view: where it starts in the frame and how long it is (the frame's units), its bins (null: not
 *  known yet), and what it is (its colour's key). */
export type StripTrack = { key: string; start: number; d: number; bins: Uint8Array | null };

/** A track's bins against its own loudest (0..1), with a sparse table of maxima: the loudest of any run of them at once.
 *  Kept by the bins themselves (a track's bins are made once, lib/waveforms). */
type Table = { v: Float32Array; st: Float32Array[] };
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
  const t = { v, st };
  tables.set(bins, t);
  return t;
}
/** The loudest of bins `a` to `b` (inclusive). */
function runMax(t: Table, a: number, b: number): number {
  const k = Math.floor(Math.log2(b - a + 1));
  return Math.max(t.st[k][a], t.st[k][b - (1 << k) + 1]);
}

/**
 * The music's loudest from `a` to `b` of the frame (0..1, each track against its own loudest; 0 where none): the bins
 * that stretch touches; one finer than a bin, between the two nearest bins at its middle.
 */
export function loudness(tracks: readonly StripTrack[], a: number, b: number): number {
  let m = 0;
  for (const t of tracks) {
    if (!t.bins || !t.bins.length || t.d <= 0 || b <= t.start || a >= t.start + t.d) continue;
    const tab = tableOf(t.bins);
    const L = t.bins.length;
    const per = L / t.d;
    const x0 = (Math.max(a, t.start) - t.start) * per;
    const x1 = (Math.min(b, t.start + t.d) - t.start) * per;
    if (x1 - x0 < 1) {
      const c = Math.min(L - 1, Math.max(0, (x0 + x1) / 2 - 0.5));
      const j = Math.floor(c);
      const k = Math.min(L - 1, j + 1);
      m = Math.max(m, tab.v[j] + (tab.v[k] - tab.v[j]) * (c - j));
    } else m = Math.max(m, runMax(tab, Math.max(0, Math.floor(x0)), Math.min(L - 1, Math.ceil(x1) - 1)));
  }
  return m;
}

/** A level's bars `unit` · 2^L apart, bar `g` from g·pitch to (g+1)·pitch of the frame. */
export const pitchOf = (L: number, unit: number) => unit * 2 ** L;

/**
 * The two levels a stretch `width` wide in `n` pieces shows: the finer (`fine`, its bars between one and two a piece)
 * and the next coarser, and how far toward the coarser the zoom is (`t`, 0..1), so that the pieces shown stay about `n`
 * as it zooms.
 */
export function levelsOf(width: number, n: number, unit: number): { fine: number; t: number } {
  const x = Math.log2(Math.max(1e-12, width / Math.max(1, n)) / unit);
  const fine = Math.floor(x);
  return { fine, t: x - fine };
}

/** How much of each level shows at `t`: the finer 1 - s, the coarser s, s easing in and out across the octave's middle
 *  (each alone near its own end). */
export function mixOf(t: number): number {
  const u = Math.min(1, Math.max(0, (t - 0.2) / 0.6));
  return u * u * (3 - 2 * u);
}

/** A level's strip as drawn: its level and pitch, its first bar and how many, and the stretch of the frame they cover. */
export type LevelStrip = { L: number; pitch: number; g0: number; count: number; e0: number; e1: number };

/** The strip of level `L` to draw for the stretch `from`..`to`: STRIP_SPAN times it, the stretch in its middle, on the
 *  level's own grid. */
export function levelCover(L: number, unit: number, from: number, to: number): LevelStrip {
  const pitch = pitchOf(L, unit);
  const w = to - from;
  const g0 = Math.floor((from - ((STRIP_SPAN - 1) / 2) * w) / pitch);
  const g1 = Math.ceil((to + ((STRIP_SPAN - 1) / 2) * w) / pitch);
  return { L, pitch, g0, count: Math.max(1, g1 - g0), e0: g0 * pitch, e1: g1 * pitch };
}

/** Whether a strip as drawn still covers the stretch. */
export const covers = (s: LevelStrip, from: number, to: number) => from >= s.e0 && to <= s.e1;

/** A strip's bars, 0..1 each: the music's loudest under each (a function of its place alone). */
export function levelBars(tracks: readonly StripTrack[], s: LevelStrip): number[] {
  return Array.from({ length: s.count }, (_, i) => loudness(tracks, (s.g0 + i) * s.pitch, (s.g0 + i + 1) * s.pitch));
}

/** Which track each bar's middle is in (its index in `tracks`; -1: none), for its colour. */
export function barTracks(tracks: readonly StripTrack[], s: LevelStrip): number[] {
  return Array.from({ length: s.count }, (_, i) => {
    const x = (s.g0 + i + 0.5) * s.pitch;
    return tracks.findIndex((t) => x >= t.start && x < t.start + t.d);
  });
}

/** Where a strip stands for the stretch on a waveform `width` px wide, drawn as wide as the waveform: scaled by `k` from
 *  its left, then slid by `x` px, so that `from` is at the waveform's left and `to` at its right. */
export function placeOf(s: LevelStrip, from: number, to: number, width: number): { x: number; k: number } {
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
export function stripSource(tracks: readonly StripTrack[], s: LevelStrip): string {
  return tracks
    .filter((t) => t.start < s.e1 && t.start + t.d > s.e0)
    .map((t) => `${t.key}@${Math.round(t.start * 1e6)}+${Math.round(t.d * 1e6)}:${binsId(t.bins)}`)
    .join(',');
}

/**
 * The strips to show for the stretch `from`..`to` in `n` pieces, from those shown (`had`): the levels the zoom asks for
 * (lib/waveStrip's levelsOf), each the strip already shown at that level where it still covers the stretch and was drawn
 * from the same tracks (the same object), else a new one to draw; and how much of the coarser shows (`mix`).
 */
export function planStrips<S extends LevelStrip & { src: string }>(
  had: readonly S[],
  tracks: readonly StripTrack[],
  from: number,
  to: number,
  n: number,
  unit: number,
): { strips: (S | (LevelStrip & { src: string }))[]; mix: number } {
  const { fine, t } = levelsOf(to - from, n, unit);
  const mix = mixOf(t);
  const levels = [fine, fine + 1].filter((_, i) => (i === 0 ? mix < 1 : mix > 0));
  const strips = levels.map((L) => {
    const kept = had.find((d) => d.L === L);
    if (kept && covers(kept, from, to) && kept.src === stripSource(tracks, kept)) return kept;
    const c = levelCover(L, unit, from, to);
    return { ...c, src: stripSource(tracks, c) };
  });
  return { strips, mix };
}
