// The Grid (decision 0057 §4c; TouchDesigner's Grid SOP): points on a lattice, centred, and each point's values as
// polyphonic channels, one a point (VCV's polyphony), for Instance to draw a source at and for cables to move knobs by.
// Pure: the same knobs and seed give the same points, every frame.
//
// - Lattices, each a standard one, `spacing` the distance between neighbouring points:
//   - square: rows and columns, as near a square as the count allows;
//   - hexagonal: the centres of a hexagonal tiling (rows half a step apart in turn, each point six neighbours);
//   - triangular: the centres of a triangular tiling (its triangles in turn up and down, each point three
//     neighbours: the honeycomb's points);
//   - rings: polar rings round a centre point, the k-th with 6k points (fewer on the last, spread evenly round it);
//   - spiral: phyllotaxis, Vogel's model (H. Vogel, "A better way to construct the sunflower head", Mathematical
//     Biosciences 44, 1979): point n at the golden angle (π(3 − √5), 137.50776…°) from the last, at a radius growing
//     as √n, each point the area a square lattice's has.
// - The whole is centred on the middle of its extent, a short last row in the middle of its places (so a full square,
//   and a hexagonal one of an even number of rows, is symmetric through the centre).
// - Rotation turns the whole (in turns); jitter moves each point up to half the spacing in a direction of its own,
//   seeded (the module's id through flame.ts `seedOf`, the caller's), so it is the same every frame.

import { LATTICES, MOST_POINTS } from '../ports';
import { point } from '../modulators/modulators';

/** π(3 − √5): the golden angle in radians (137.50776…°). */
export const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

/** The Grid's knobs as ports.ts's GRID declares them: `lattice` a step into LATTICES, `count` points (1 … 64),
 *  `spacing` between neighbours (the picture's short side spanning −1..1), `rotation` in turns, `jitter` 0..1 (1: up
 *  to half the spacing). */
export type GridKnobs = { lattice: number; count: number; spacing: number; rotation: number; jitter: number };

export type GridPoint = { x: number; y: number };

/** A point's values, each 0..1 as a cable reads them: its place in the order (`index`: 0 the first, 1 the last), its
 *  distance from the centre (1 the farthest point's), its angle about the centre (turns, counter-clockwise from the
 *  right), its row and its column (0 the first, 1 the last; on rings the ring and the place round it; on the spiral its
 *  arm in each of the two families of spirals seen at up to 64 points, the 8 and the 13 of Fibonacci). */
export type GridChannels = { index: number; distance: number; angle: number; row: number; column: number };

export type Grid = { points: GridPoint[]; channels: GridChannels[] };

const DEFAULTS: GridKnobs = { lattice: 0, count: 16, spacing: 0.25, rotation: 0, jitter: 0 };
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const knobOf = (v: number | undefined, or: number) => (typeof v === 'number' && Number.isFinite(v) ? v : or);
const share = (i: number, n: number) => (n > 1 ? i / (n - 1) : 0);

/** The spiral's two families of arms (parastichies) at up to 64 points: Fibonacci's 8 and 13. */
const ARMS = [8, 13] as const;

type Laid = { x: number; y: number; row: number; rows: number; column: number; columns: number };

/** `n` points in rows of `columns`, a row `rowStep` under the last, its places `step` apart, the place `c` of row `r`
 *  moved by `shift(r, c)`; a short last row in the middle of its places (by whole places, so it stays on the
 *  lattice), the whole centred on the middle of its extent. */
function rows(n: number, columns: number, step: number, rowStep: number, shift: (r: number, c: number) => GridPoint): Laid[] {
  const rowCount = Math.ceil(n / columns);
  const out: Laid[] = [];
  for (let r = 0; r < rowCount; r++) {
    const inRow = Math.min(columns, n - r * columns);
    const from = Math.floor((columns - inRow) / 2);
    for (let c = from; c < from + inRow; c++) {
      const d = shift(r, c);
      out.push({ x: (c - (columns - 1) / 2) * step + d.x, y: -r * rowStep + d.y, row: r, rows: rowCount, column: c, columns });
    }
  }
  const xs = out.map((p) => p.x);
  const ys = out.map((p) => p.y);
  const cx = (Math.min(...xs) + Math.max(...xs)) / 2;
  const cy = (Math.min(...ys) + Math.max(...ys)) / 2;
  for (const p of out) {
    p.x -= cx;
    p.y -= cy;
  }
  return out;
}

/** Columns for `n` points whose rows are `rowStep` apart and points `step` apart, so the whole is about as tall as wide. */
const columnsFor = (n: number, step: number, rowStep: number) => Math.max(1, Math.min(n, Math.ceil(Math.sqrt((n * rowStep) / step))));

function lay(lattice: (typeof LATTICES)[number], n: number, s: number): Laid[] {
  switch (lattice) {
    case 'square':
      return rows(n, columnsFor(n, s, s), s, s, () => ({ x: 0, y: 0 }));
    case 'hexagonal': {
      // Rows √3/2 apart, every other one half a step along: each point's six neighbours a spacing away.
      const h = (s * Math.sqrt(3)) / 2;
      return rows(n, columnsFor(n, s, h), s, h, (r) => ({ x: r % 2 ? s / 2 : 0, y: 0 }));
    }
    case 'triangular': {
      // Triangles of side s√3 (their centres a spacing from their three neighbours'), up and down in turn along a row:
      // centres half a side apart along it, an up one's a third of the height above its row's base, a down one's two.
      const side = s * Math.sqrt(3);
      const height = (side * Math.sqrt(3)) / 2;
      const columns = columnsFor(n, side / 2, height);
      return rows(n, columns, side / 2, height, (r, c) => ({ x: 0, y: ((r + c) % 2 === 0 ? 1 : 2) * (height / 3) - height / 2 }));
    }
    case 'rings': {
      const out: Laid[] = [];
      const ringsOf: number[] = [1];
      for (let left = n - 1, k = 1; left > 0; k++) {
        ringsOf.push(Math.min(6 * k, left));
        left -= 6 * k;
      }
      ringsOf.forEach((m, k) => {
        for (let j = 0; j < m; j++) {
          const a = (2 * Math.PI * j) / m;
          out.push({ x: k * s * Math.cos(a), y: k * s * Math.sin(a), row: k, rows: ringsOf.length, column: j, columns: m });
        }
      });
      return out;
    }
    case 'spiral': {
      // Each point the area s² (a square lattice's): πr² = n s².
      const c = s / Math.sqrt(Math.PI);
      return Array.from({ length: n }, (_, i) => {
        const r = c * Math.sqrt(i);
        const a = i * GOLDEN_ANGLE;
        return { x: r * Math.cos(a), y: r * Math.sin(a), row: i % ARMS[0], rows: ARMS[0], column: i % ARMS[1], columns: ARMS[1] };
      });
    }
  }
}

/** The Grid's points and their channels, for these knobs (each its default where not a number) and `seed` (the
 *  module's id through `seedOf`). */
export function gridOf(knobs: Partial<GridKnobs>, seed: number): Grid {
  const lattice = LATTICES[clamp(Math.round(knobOf(knobs.lattice, DEFAULTS.lattice)), 0, LATTICES.length - 1)];
  const n = clamp(Math.round(knobOf(knobs.count, DEFAULTS.count)), 1, MOST_POINTS);
  const s = clamp(knobOf(knobs.spacing, DEFAULTS.spacing), 0.02, 1);
  const turn = 2 * Math.PI * knobOf(knobs.rotation, DEFAULTS.rotation);
  const jitter = clamp(knobOf(knobs.jitter, DEFAULTS.jitter), 0, 1);
  const laid = lay(lattice, n, s);
  const cos = Math.cos(turn);
  const sin = Math.sin(turn);
  const points = laid.map((p, i): GridPoint => {
    let { x, y } = p;
    if (jitter > 0) {
      // A direction and a distance of its own, uniform over the disc of half a spacing.
      const a = Math.PI * point(seed, 2 * i);
      const r = (jitter * s * Math.sqrt((point(seed, 2 * i + 1) + 1) / 2)) / 2;
      x += r * Math.cos(a);
      y += r * Math.sin(a);
    }
    return turn ? { x: x * cos - y * sin, y: x * sin + y * cos } : { x, y };
  });
  const far = Math.max(...points.map((p) => Math.hypot(p.x, p.y)));
  const channels = points.map((p, i): GridChannels => {
    const angle = Math.atan2(p.y, p.x) / (2 * Math.PI);
    return {
      index: share(i, n),
      distance: far > 0 ? Math.hypot(p.x, p.y) / far : 0,
      angle: angle < 0 ? angle + 1 : angle,
      row: share(laid[i].row, laid[i].rows),
      column: share(laid[i].column, laid[i].columns),
    };
  });
  return { points, channels };
}
