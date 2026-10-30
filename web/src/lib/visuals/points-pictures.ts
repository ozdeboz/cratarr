// Instance and Fold's points (decision 0057 §4c): a Grid's (engine/grid.ts: centred, y up, the picture's short side
// −1..1, the square space of transform.ts), as the passes take them. Pure: no GL here (nodes/points-pictures.ts).

export type Point = { x: number; y: number };

/** The most points an Instance or a Fold reads (a Grid's most). */
export const MOST = 64;

/** The cell each point has: the median of the points' distances to their nearest neighbours (a lattice's spacing; points
 *  a flow carries keep their size as two of them meet, their cells overlapping where they do); a lone point the whole
 *  short side. */
export function spacingOf(points: readonly Point[]): number {
  const n = Math.min(points.length, MOST);
  const nearest: number[] = [];
  for (let i = 0; i < n; i++) {
    let best = Infinity;
    for (let j = 0; j < n; j++) {
      if (i === j) continue;
      const d = Math.hypot(points[i].x - points[j].x, points[i].y - points[j].y);
      if (d > 1e-6 && d < best) best = d;
    }
    if (Number.isFinite(best)) nearest.push(best);
  }
  if (!nearest.length) return 2;
  nearest.sort((a, b) => a - b);
  return Math.min(2, nearest[nearest.length >> 1]);
}

/** Up to `MOST` points as x, y pairs (into `out`, made once), and how many. */
export function packed(points: readonly Point[] | null | undefined, out: Float32Array, most = MOST): number {
  const n = Math.min(points?.length ?? 0, most, out.length >> 1);
  for (let i = 0; i < n; i++) {
    out[i * 2] = points![i].x;
    out[i * 2 + 1] = points![i].y;
  }
  return n;
}

/** The point nearest `p` (its index; -1 for none), as Fold's shader finds it for its cells. */
export function nearest(points: readonly Point[], p: Point): number {
  let best = -1;
  let d = Infinity;
  points.slice(0, MOST).forEach((q, i) => {
    const e = (q.x - p.x) ** 2 + (q.y - p.y) ** 2;
    if (e < d) [d, best] = [e, i];
  });
  return best;
}
