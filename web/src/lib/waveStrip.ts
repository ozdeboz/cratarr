// A hold's waveform as one strip (lib/Waveform; the owner, 2026-10-29: a held wind went in strides, the tablet redrawing
// every bar of four layers each frame): drawn once for three times the stretch shown, then slid and scaled by the
// compositor as the stretch moves, drawn anew only once the stretch leaves it or its scale is a quarter off. Places are
// 0..1 of the source's bins (the track's, or the queue's around it).
// Drawn anew it shows no step (0.10.1, the owner: "So pure continuous transitions"): its bars stand on one grid of the
// source for each of a ladder of scales, each bar a function of its place on that grid alone (its height and its
// range), so a strip drawn anew at the same scale has the same bars where the last one had them; only a change of
// scale (or of source) draws other bars, and lib/Waveform cross-fades those.

/** How much wider than the stretch shown a strip is drawn, and how far its scale may go before it is drawn anew. */
export const STRIP_SPAN = 3;
export const STRIP_SCALE = 1.25;

/** What a strip covers (`e0`..`e1`), the width of the stretch shown it was drawn for (`w0`, a step of the ladder), and
 *  its first bar's place on that step's grid (`g0`, the bars `pitch` apart). */
export type Drawn = { e0: number; e1: number; w0: number; g0: number; pitch: number };

/** The ladder's step nearest a stretch `w` wide: a power of STRIP_SCALE. */
export function stepOf(w: number): number {
  return STRIP_SCALE ** Math.round(Math.log(Math.max(1e-9, w)) / Math.log(STRIP_SCALE));
}

/** What a strip of `count` bars drawn for the stretch `from`..`to` covers: the step's width on either side of it, from
 *  a bar of its grid. */
export function coverOf(from: number, to: number, count: number): Drawn {
  const w0 = stepOf(to - from);
  const pitch = (STRIP_SPAN * w0) / count;
  const g0 = Math.floor((from - w0) / pitch);
  return { e0: g0 * pitch, e1: (g0 + count) * pitch, w0, g0, pitch };
}

/** Whether a strip as drawn still serves the stretch: within what it covers, its scale within a quarter. */
export function serves(d: Drawn, from: number, to: number): boolean {
  const k = d.w0 / Math.max(1e-6, to - from);
  return k >= 1 / STRIP_SCALE && k <= STRIP_SCALE && from >= d.e0 && to <= d.e1;
}

/** Where the strip stands for the stretch on a waveform `width` px wide: scaled by `k` from its left, then slid by `x`
 *  px, so that `from` is at the waveform's left and `to` at its right. */
export function placeOf(d: Drawn, from: number, to: number, width: number): { x: number; k: number } {
  const k = d.w0 / Math.max(1e-6, to - from);
  return { x: -k * ((from - d.e0) / d.w0) * width, k };
}

/**
 * The strip's bars, 0..1 each, against the source's loudest: bar `g` of the grid `pitch` apart covers `g`·pitch to
 * (`g`+1)·pitch of the source: its loudest bin there, or between the two nearest where a bar is finer than a bin; none
 * outside the source. By `contrast` (0..1) toward its own range, its quietest at the floor and its loudest at the
 * ceiling: the range of the bars within `reach` of it on either side (the stretch shown about it), so that a bar's
 * height is its place's alone.
 */
export function gridBars(bins: Uint8Array, d: Drawn, count: number, contrast = 0, reach = 0): number[] {
  const L = bins.length;
  let top = 1;
  for (let j = 0; j < L; j++) top = Math.max(top, bins[j]);
  const per = d.pitch * L;
  const r = contrast > 0 ? Math.max(0, Math.round(reach)) : 0;
  const bar = (g: number) => {
    const a = g * per;
    if (a + per <= 0 || a >= L) return 0;
    if (per < 1) {
      const x = a + per / 2 - 0.5;
      if (x < -0.5 || x > L - 0.5) return 0;
      const c = Math.min(L - 1, Math.max(0, x));
      const j = Math.floor(c);
      const k = Math.min(L - 1, j + 1);
      return (bins[j] + (bins[k] - bins[j]) * (c - j)) / top;
    }
    let m = 0;
    for (let j = Math.max(0, Math.floor(a)); j < Math.min(L, Math.ceil(a + per)); j++) m = Math.max(m, bins[j]);
    return m / top;
  };
  // The bars with `r` beyond either end, for each one's range.
  const all = Array.from({ length: count + 2 * r }, (_, i) => bar(d.g0 - r + i));
  if (r === 0) return all;
  const out: number[] = [];
  for (let i = 0; i < count; i++) {
    let lo = Infinity;
    let hi = -Infinity;
    for (let j = i; j <= i + 2 * r; j++) {
      lo = Math.min(lo, all[j]);
      hi = Math.max(hi, all[j]);
    }
    const v = all[i + r];
    out.push(hi - lo < 1e-3 ? v : v + ((v - lo) / (hi - lo) - v) * contrast);
  }
  return out;
}
