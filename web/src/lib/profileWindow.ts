// The list filter's window lit on its profile's bars (the owner, 2026-11-02: "the filter's window lit on the panel's
// bars"): each range condition of the filter, on the bar of its measure. Energy and danceability are drawn on the
// library's percentiles (the server's `percentiles`, the measure's value at each hundredth), so a range in the
// measure's own values is placed through them; tempo and year are drawn in BPM and years. Pure, for the tests.
import type { Condition, ListProfileData } from './api';

type NumberRow = ListProfileData['numbers'][number];
/** A window in the measure's own values; null: open at that end. */
export type Window = [number | null, number | null];

/** The crate axis each profile row is of. */
const AXIS_OF: Record<NumberRow['key'], string> = { tempo: 'bpm', energy: 'energy', danceability: 'danceability', year: 'year' };

/** Where `v` falls among the library's hundredths (`table`, 101 values ascending), 0 to 100, between two in proportion
 *  (as the server's `percent_in`). */
export function percentIn(table: readonly number[], v: number): number {
  let above = 0;
  while (above < table.length && table[above] <= v) above++;
  if (above === 0) return 0;
  if (above >= table.length) return 100;
  const [a, b] = [table[above - 1], table[above]];
  return above - 1 + (b > a ? (v - a) / (b - a) : 0);
}

/** The filter's window on each profile row it has a range for: its ranges on that axis taken together (each narrows
 *  it); a condition left out (`not`) is no window. */
export function windowsOf(conditions: readonly Condition[] | undefined): Partial<Record<NumberRow['key'], Window>> {
  const out: Partial<Record<NumberRow['key'], Window>> = {};
  for (const [key, axis] of Object.entries(AXIS_OF) as [NumberRow['key'], string][])
    for (const c of conditions ?? []) {
      if (c.axis !== axis || c.not || c.value.kind !== 'range') continue;
      const [lo, hi] = out[key] ?? [null, null];
      const { min, max } = c.value;
      out[key] = [min === null ? lo : lo === null ? min : Math.max(lo, min), max === null ? hi : hi === null ? max : Math.min(hi, max)];
    }
  return out;
}

/** A window on row `n`'s bar, 0 to 1 of its scale (an open end at the bar's end); null where it has none. */
export function insideOf(n: NumberRow, w: Window | undefined): [number, number] | null {
  if (!w) return null;
  const at = (v: number) => (n.percentiles ? percentIn(n.percentiles, v) : v);
  const of = (v: number) => Math.min(1, Math.max(0, (at(v) - n.lo) / (n.hi - n.lo || 1)));
  const lo = w[0] === null ? 0 : of(w[0]);
  const hi = w[1] === null ? 1 : of(w[1]);
  return [lo, Math.max(lo, hi)];
}
