// Grids of items in columns (the playlist chips, the album lists): an item's least width sets how many columns fit.

/** The gap between a grid's items (px), as their CSS has it. */
export const GAP = 10;

/** How many columns of items at least `min` px wide fit in `width`. */
export const columns = (width: number, min: number) => Math.max(1, Math.floor((width + GAP) / (min + GAP)));

/** From size `at` (an index into `widths`), the next one in direction `dir` that changes the columns at `width`; null
 *  when none would. */
export function nextZoom(widths: readonly number[], width: number, at: number, dir: -1 | 1): number | null {
  for (let i = at + dir; i >= 0 && i < widths.length; i += dir) if (columns(width, widths[i]) !== columns(width, widths[at])) return i;
  return null;
}
