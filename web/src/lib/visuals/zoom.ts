// The infinite zooms (decision 0050; the owner, after trying them: "I want both"). Pure: the renderer (render.ts) runs
// them, the visuals (aspects.ts) say how fast.
//
// - The tunnel: the light kept from frame to frame is read a little nearer the middle each frame, so what was drawn is
//   drawn larger and streams outward for ever; the flame itself keeps its size.
// - The dive: the camera zooms toward the fixed point of the flame's part that is most nearly a turned, shrunk copy of
//   the whole, by that part's own shrink and turn. A flame is the union of its parts' copies of itself, so after one
//   loop the part has become the whole again: the zoom never ends. A share of the points, growing through the loop, is
//   drawn as that part's copy of them, so the part is as densely drawn at the end of the loop as the whole at its
//   start. Where the part is not exactly a turn and shrink (its variations bend it), the picture morphs a little as a
//   loop starts again: the fit's misfit.

import { apply, picker, randomFrom, type Flame } from './flame';

/** The fastest each zoom goes at its slider's top (its log a second: 0.5 is 65 % larger a second). */
export const TUNNEL_MOST = 0.5;
export const DIVE_MOST = 0.5;

/** A part fit as a turn and shrink of the plane, `w ≈ a·z + b` in complex numbers; `error`: how far off (the misfit
 *  against the sample's spread). */
export type DiveFit = { a: [number, number]; b: [number, number]; error: number };

/** How nearly part `j` of a flame is a turn and shrink of the plane: the a, b best mapping a sample of the flame's
 *  points onto that part's copies of them (least squares). Null where there is no such part or too few points. */
export function diveFit(flame: Flame, j: number, n = 1200, seed = 4242): DiveFit | null {
  const xf = flame.xforms[j];
  if (!xf) return null;
  const rnd = randomFrom(seed);
  const pick = picker(flame.xforms);
  let x = 0.1;
  let y = 0.1;
  const zs: number[] = [];
  const ws: number[] = [];
  for (let i = 0; i < n + 30; i++) {
    const p = apply(flame.xforms[pick(rnd())], x, y, rnd);
    if (!Number.isFinite(p[0]) || !Number.isFinite(p[1]) || Math.abs(p[0]) > 1e5 || Math.abs(p[1]) > 1e5) {
      x = rnd() * 2 - 1;
      y = rnd() * 2 - 1;
      continue;
    }
    [x, y] = p;
    if (i < 30) continue;
    const w = apply(xf, x, y, rnd);
    if (!Number.isFinite(w[0]) || !Number.isFinite(w[1])) continue;
    zs.push(x, y);
    ws.push(w[0], w[1]);
  }
  const m = zs.length / 2;
  if (m < 100) return null;
  let zx = 0;
  let zy = 0;
  let wx = 0;
  let wy = 0;
  for (let i = 0; i < m; i++) {
    zx += zs[2 * i];
    zy += zs[2 * i + 1];
    wx += ws[2 * i];
    wy += ws[2 * i + 1];
  }
  zx /= m;
  zy /= m;
  wx /= m;
  wy /= m;
  let nr = 0;
  let ni = 0;
  let den = 0;
  for (let i = 0; i < m; i++) {
    const dzx = zs[2 * i] - zx;
    const dzy = zs[2 * i + 1] - zy;
    const dwx = ws[2 * i] - wx;
    const dwy = ws[2 * i + 1] - wy;
    nr += dwx * dzx + dwy * dzy;
    ni += dwy * dzx - dwx * dzy;
    den += dzx * dzx + dzy * dzy;
  }
  if (den < 1e-12) return null;
  const a: [number, number] = [nr / den, ni / den];
  const b: [number, number] = [wx - (a[0] * zx - a[1] * zy), wy - (a[1] * zx + a[0] * zy)];
  let miss = 0;
  for (let i = 0; i < m; i++) {
    const px = a[0] * zs[2 * i] - a[1] * zs[2 * i + 1] + b[0];
    const py = a[1] * zs[2 * i] + a[0] * zs[2 * i + 1] + b[1];
    miss += (ws[2 * i] - px) ** 2 + (ws[2 * i + 1] - py) ** 2;
  }
  return { a, b, error: Math.sqrt(miss / den) };
}

/** The part to dive into: of those shrinking the whole to between a fifth and nine tenths, the one fitting best (the
 *  least morph as a loop starts again). */
export function diveTarget(flame: Flame): ({ j: number } & DiveFit) | null {
  let best: ({ j: number } & DiveFit) | null = null;
  for (let j = 0; j < flame.xforms.length; j++) {
    const fit = diveFit(flame, j, 600);
    if (!fit) continue;
    const shrink = Math.hypot(fit.a[0], fit.a[1]);
    if (shrink < 0.2 || shrink > 0.9) continue;
    if (!best || fit.error < best.error) best = { j, ...fit };
  }
  if (!best) return null;
  const fine = diveFit(flame, best.j);
  return fine ? { j: best.j, ...fine } : best;
}

/** How long one loop takes at `rate` (the zoom's log a second): as long as the zoom takes to grow by the part's shrink. */
export const divePeriod = (fit: DiveFit, rate: number) => Math.log(1 / Math.hypot(fit.a[0], fit.a[1])) / Math.max(1e-3, rate);

/** The camera at `t` into the loop (0..1): the zoom (a complex factor, a^{-t}, growing by 1/shrink over the loop and
 *  turning back the part's turn) about the part's fixed point (b / (1 − a)); and how fast it turns (radians a second
 *  at `rate`). */
export function diveCamera(fit: DiveFit, t: number, rate: number): { zoom: [number, number]; at: [number, number]; turnRate: number } {
  const shrink = Math.hypot(fit.a[0], fit.a[1]);
  const turn = Math.atan2(fit.a[1], fit.a[0]);
  const m = Math.exp(-t * Math.log(shrink));
  const den = (1 - fit.a[0]) ** 2 + fit.a[1] ** 2;
  return {
    zoom: [m * Math.cos(-t * turn), m * Math.sin(-t * turn)],
    at: [(fit.b[0] * (1 - fit.a[0]) - fit.b[1] * fit.a[1]) / den, (fit.b[1] * (1 - fit.a[0]) + fit.b[0] * fit.a[1]) / den],
    turnRate: -turn / divePeriod(fit, rate),
  };
}

/** How fast a dive at `rate` would turn the picture for this flame (radians a second, unsigned): the dive's turn,
 *  foreseen from the part it would dive into, for the precheck to count as the live sidechaining does
 *  (`renderer.diveTurnRate`). 0 where there is no dive or no part to dive into. */
export function diveTurnOf(flame: Flame, rate: number): number {
  if (!(rate > 0)) return 0;
  const target = diveTarget(flame);
  return target ? Math.abs(diveCamera(target, 0, rate).turnRate) : 0;
}
