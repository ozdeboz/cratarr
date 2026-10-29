// A hold's waveform as one strip (lib/Waveform; the owner, 2026-10-29: a held wind went in strides, the tablet redrawing
// every bar of four layers each frame): drawn once for three times the stretch shown, then slid and scaled by the
// compositor as the stretch moves, drawn anew only once the stretch leaves it or its scale is a quarter off. Places are
// 0..1 of the source's bins (the track's, or the queue's around it).

/** How much wider than the stretch shown a strip is drawn, and how far its scale may go before it is drawn anew. */
export const STRIP_SPAN = 3;
export const STRIP_SCALE = 1.25;

/** What a strip covers (`e0`..`e1`) and the width of the stretch shown when it was drawn (`w0`). */
export type Drawn = { e0: number; e1: number; w0: number };

/** What a strip drawn for the stretch `from`..`to` covers: the stretch's width on either side of it. */
export function coverOf(from: number, to: number): Drawn {
  const w = Math.max(1e-6, to - from);
  return { e0: from - w, e1: to + w, w0: w };
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
