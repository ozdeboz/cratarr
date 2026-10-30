// A route's curve (decision 0046, amended for 64.1; the owner: "Yes, every route"): what its signal (across, 0..1) gives
// its aspect (up, −1..1), as a cubic Bézier from (0, 0) through two handles to its end at (1, h). A plain number is the
// straight one ending at it, a route's amount as it always was, so every route kept, shared or saved before still reads
// the same. Pure.
//
// With both handles' x within 0..1 the curve's x only ever grows along it (the derivative's Bernstein coefficients x1,
// x2 − x1 and 1 − x2 make a quadratic that never dips below 0), so each signal has one height; and the curve stays
// within its control points' hull, so within −1..1.

/** `walks`: it walks with the behaviour (the Behaviour column's walk), only ever at or under itself (the owner: "behavior
 *  should also affect those matrix routes that subscribe to it … as an attenuating walk"); off unless set. */
export type Curve = { h1: [number, number]; h2: [number, number]; h: number; walks?: boolean };
/** A route as kept: its amount (straight), or its curve. */
export type RouteValue = number | Curve;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** The straight curve ending at `a`. */
export const straight = (a: number): Curve => ({ h1: [1 / 3, a / 3], h2: [2 / 3, (2 * a) / 3], h: a });

/** A route's curve, a number as the straight one. */
export const curveOf = (v: RouteValue): Curve => (typeof v === 'number' ? straight(v) : v);

/** Where a route ends (its amount, as the grid shows it). */
export const endOf = (v: RouteValue) => (typeof v === 'number' ? v : v.h);

/** Whether a curve is the straight one to its end (handles where a straight line's are, within a hair). */
export function isStraight(c: Curve): boolean {
  const s = straight(c.h);
  const near = (a: [number, number], b: [number, number]) => Math.abs(a[0] - b[0]) < 1e-4 && Math.abs(a[1] - b[1]) < 1e-4;
  return near(c.h1, s.h1) && near(c.h2, s.h2);
}

/** A curve as kept: a number where it is straight and does not walk. */
export const valueOf = (c: Curve): RouteValue => (isStraight(c) && !c.walks ? c.h : c);

/** Whether a route walks with the behaviour. */
export const walksOf = (v: RouteValue) => typeof v !== 'number' && !!v.walks;

/** A curve brought into its ranges (handles' x 0..1, heights −1..1), or null where it is not one. */
export function curveFrom(v: unknown): Curve | null {
  if (!v || typeof v !== 'object') return null;
  const o = v as Record<string, unknown>;
  const pt = (p: unknown): [number, number] | null =>
    Array.isArray(p) && p.length === 2 && p.every((x) => typeof x === 'number' && Number.isFinite(x)) ? [clamp(p[0], 0, 1), clamp(p[1], -1, 1)] : null;
  const h1 = pt(o.h1);
  const h2 = pt(o.h2);
  if (!h1 || !h2 || typeof o.h !== 'number' || !Number.isFinite(o.h)) return null;
  return { h1, h2, h: clamp(o.h, -1, 1), ...(o.walks === true ? { walks: true } : {}) };
}

/** A curve ending elsewhere, its shape kept (its heights scaled; the straight one where it ended at 0). */
export const withEnd = (c: Curve, h: number): Curve =>
  Math.abs(c.h) < 1e-6
    ? { ...straight(h), ...(c.walks ? { walks: true } : {}) }
    : { h1: [c.h1[0], clamp((c.h1[1] * h) / c.h, -1, 1)], h2: [c.h2[0], clamp((c.h2[1] * h) / c.h, -1, 1)], h, ...(c.walks ? { walks: true } : {}) };

/** How many steps a table has across (its points: one more). */
export const TABLE_STEPS = 32;

/** A curve's heights at the signals 0, 1/32 … 1 (`table`), and the most it gives (`peak`: its highest, and never under
 *  0). Its points along t, finely, then each signal's height between the two it falls between. */
export function tableOf(c: Curve): { table: Float32Array; peak: number } {
  const N = 512;
  const xs = new Float64Array(N + 1);
  const ys = new Float64Array(N + 1);
  let peak = 0;
  for (let i = 0; i <= N; i++) {
    const t = i / N;
    const u = 1 - t;
    const b1 = 3 * u * u * t;
    const b2 = 3 * u * t * t;
    const b3 = t * t * t;
    xs[i] = b1 * c.h1[0] + b2 * c.h2[0] + b3;
    ys[i] = b1 * c.h1[1] + b2 * c.h2[1] + b3 * c.h;
    peak = Math.max(peak, ys[i]);
  }
  const table = new Float32Array(TABLE_STEPS + 1);
  let j = 0;
  for (let k = 0; k <= TABLE_STEPS; k++) {
    const x = k / TABLE_STEPS;
    while (j < N - 1 && xs[j + 1] < x) j++;
    const span = xs[j + 1] - xs[j];
    const f = span > 1e-12 ? clamp((x - xs[j]) / span, 0, 1) : 0;
    table[k] = ys[j] + (ys[j + 1] - ys[j]) * f;
  }
  return { table, peak };
}

/** A table's height at signal `v` (0..1). */
export function heightAt(table: Float32Array, v: number): number {
  const p = clamp(v, 0, 1) * TABLE_STEPS;
  const i = Math.min(TABLE_STEPS - 1, Math.floor(p));
  return table[i] + (table[i + 1] - table[i]) * (p - i);
}
