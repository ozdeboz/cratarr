// One set of shapes (decision 0057 §4g, the owner: "the same set of shapes everywhere shapes are used"): each shape
// defined once, here, and used by both modules that work with shapes: Regular and Irregular shapes draw them, Lens
// sends points into them. Units: a shape fits in the circle of radius 1 about its middle, y up, its first corner (where
// it has corners) at angle 0. Pure, so Node can test it; the shaders draw exactly this (sources/shapes.ts, nodes/points.ts).
//
// - `edge(…)`: how far past the shape's outline a point is (negative inside), in units.
// - `fill(t, u)`: a point inside the shape for t, u in 0..1, evenly: equal shares of t and u cover equal areas. Lens
//   takes t from a point's angle and u from its distance (`lensAt`), as the Cratarr lens always has.
//
// The distance functions for boxes, triangles and the drop are Inigo Quilez's 2D distance functions
// (iquilezles.org/articles/distfunctions2d), written in TypeScript, under his MIT licence:
//
//   Copyright © Inigo Quilez
//
//   Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
//   documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
//   rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
//   permit persons to whom the Software is furnished to do so, subject to the following conditions: The above copyright
//   notice and this permission notice shall be included in all copies or substantial portions of the Software.
//
//   THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
//   WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS
//   OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
//   OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

import { TAU, morphEdge, sidesOf } from './sources/geometry';
import { WARPS, warpOf } from './warps';

/** The shapes that morph: the polygon's sides through the fractions, the star's depth through its range. */
export const REGULAR = ['polygon', 'star'] as const;
/** The shapes a list holds, stepped through, never morphed: a real heart and Cratarr (the logo), the common symbols
 *  (the owner's pick), and the lens's warps' forms that are shapes of their own (each the form its warp makes of a
 *  disc): Polar a band, Handkerchief a slanted ellipse. The owner (2026-10-20): only distinct forms; the rest are a
 *  circle, a rounded square or the ring, and stay warps in Lens. */
export const IRREGULAR = ['heart', 'cratarr', 'ring', 'cross', 'arrow', 'crescent', 'drop', 'polar', 'handkerchief'] as const;
export type Regular = (typeof REGULAR)[number];
export type Irregular = (typeof IRREGULAR)[number];
export type ShapeName = Regular | Irregular;

export const SHAPE_LABEL = Object.fromEntries([...REGULAR, ...IRREGULAR].map((n) => [n, n[0].toUpperCase() + n.slice(1)])) as Record<ShapeName, string>;

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
const mod = (x: number, m: number) => x - m * Math.floor(x / m);

/** The angle and distance of a point as Lens reads them into t and u (the Cratarr lens's from the first). */
export function lensAt(x: number, y: number, r: number): [number, number] {
  return [(Math.atan2(y, x) + Math.PI) / (2 * Math.PI), (r / (1 + r)) ** 2];
}

/** A distance into `rings` (each an inner and outer radius) by `u` (0..1), as their areas share it. */
export function ringAt(rings: readonly (readonly [number, number])[], u: number): number {
  const total = rings.reduce((s, [a, b]) => s + b * b - a * a, 0);
  let left = Math.min(1, Math.max(0, u)) * total;
  for (const [a, b] of rings) {
    const area = b * b - a * a;
    if (left < area) return Math.sqrt(a * a + left);
    left -= area;
  }
  return rings.length ? rings[rings.length - 1][1] : 0;
}

// ---- Lens: a point into a shape ------------------------------------------------------------------------------------------

/** The Lens's shapes (flame.ts LENS_SHAPE_VARIATIONS, 100 on, in this order): the regular two, the symbols, and the
 *  real heart (Heart's lens kind, its place in the lens list kept: flam3's heart variation left the lens). Cratarr's is
 *  the mark's own; the warps' forms are their warps in Lens (a warp moves a point as itself). */
export const LENS_SHAPES = ['polygon', 'star', 'ring', 'cross', 'arrow', 'crescent', 'drop', 'heart'] as const satisfies readonly ShapeName[];

/** A point (its distance `r`) sent into a lens shape, evenly: Polygon and Star by `shape`'s sides (and Star's depth). */
export function lensInto(name: (typeof LENS_SHAPES)[number], x: number, y: number, r: number, shape = { sides: 5, depth: 0.5 }): [number, number] {
  const [t, u] = lensAt(x, y, r);
  if (name === 'polygon') return regularFill(t, u, shape.sides, 0);
  if (name === 'star') return regularFill(t, u, shape.sides, shape.depth);
  return irregularFill(name, t, u);
}

// ---- Regular: polygon and star -----------------------------------------------------------------------------------

/** How deep a star goes at depth 1: its inner corners a twentieth of the way out from the middle of the polygon's
 *  sides, so it keeps an area. */
const DEEPEST = 0.95;

/** A star's side in its first half sector: from its corner (1, 0) to its inner corner at half a sector, whose distance
 *  is the polygon's side's middle at depth 0 (the star is the polygon) and nearer the middle as it deepens. Its unit
 *  normal (outwards) and its distance from the middle. */
function sideOf(n: number, depth: number): { nx: number; ny: number; c: number } {
  const h = Math.PI / n;
  const rho = Math.cos(h) * (1 - DEEPEST * clamp01(depth));
  const ex = rho * Math.cos(h) - 1;
  const ey = rho * Math.sin(h);
  const len = Math.hypot(ey, ex);
  const nx = ey / len;
  const ny = -ex / len;
  return { nx, ny, c: nx };
}

/** An angle folded into the first half sector of `n` (0 at a corner, π/n between two). */
function folded(theta: number, n: number): number {
  const s = TAU / n;
  const p = mod(theta, s);
  return p > s / 2 ? s - p : p;
}

const starEdgeOf = (r: number, theta: number, n: number, depth: number) => {
  const sd = sideOf(n, depth);
  const p = folded(theta, n);
  return r * (Math.cos(p) * sd.nx + Math.sin(p) * sd.ny) - sd.c;
};

/** A regular shape's edge at (r, θ): the polygon (geometry.ts's, exactly) at depth 0, else the star; a fractional
 *  number of sides the two whole shapes' edges mixed, as the polygon's always were. */
export function regularEdge(r: number, theta: number, sides: number, depth: number): number {
  if (!(depth > 0)) return morphEdge(r, theta, sides, 1);
  const [n0, f] = sidesOf(sides);
  return (1 - f) * starEdgeOf(r, theta, n0, depth) + f * starEdgeOf(r, theta, n0 + 1, depth);
}

/** Where a regular shape's outline is at θ: the edge is linear in r, so its zero is exact. */
export function regularOutline(theta: number, sides: number, depth: number): number {
  const [n0, f] = sidesOf(sides);
  let c = 0;
  let d = 0;
  for (const [n, w] of [
    [n0, 1 - f],
    [n0 + 1, f],
  ]) {
    if (!w) continue;
    const sd = sideOf(n, depth);
    const p = folded(theta, n);
    c += w * sd.c;
    d += w * (Math.cos(p) * sd.nx + Math.sin(p) * sd.ny);
  }
  return c / d;
}

/** The angle for t of a whole shape of `n` corners, evenly: its 2n half sectors (triangles from the middle) are of one
 *  area, and in one the area to an angle ψ from its corner is (c²/2)(tan(ψ − α) + tan α), α its side's normal's angle. */
function angleOf(t: number, n: number, depth: number): number {
  const sd = sideOf(n, depth);
  const h = Math.PI / n;
  const a = Math.atan2(sd.ny, sd.nx);
  const ta = Math.tan(a);
  const whole = Math.tan(h - a) + ta;
  const psi = (tau: number) => a + Math.atan(tau * whole - ta);
  const halves = 2 * n;
  const x = clamp01(t) * halves;
  const k = Math.min(halves - 1, Math.floor(x));
  const tau = x - k;
  const at = (k >> 1) * 2 * h;
  return k % 2 === 0 ? at + psi(tau) : at + 2 * h - psi(1 - tau);
}

/** A point inside a regular shape for t, u (evenly at whole sides; between them the two whole shapes' angles mixed). */
export function regularFill(t: number, u: number, sides: number, depth: number): [number, number] {
  const [n0, f] = sidesOf(sides);
  const theta = f > 0 ? (1 - f) * angleOf(t, n0, depth) + f * angleOf(t, n0 + 1, depth) : angleOf(t, n0, depth);
  const r = regularOutline(theta, sides, depth) * Math.sqrt(clamp01(u));
  return [r * Math.cos(theta), r * Math.sin(theta)];
}

/** A regular shape's corners, at radius 1 turned by `turn` (radians): as many as its sides round to. */
export function regularCorners(sides: number, depth: number, turn = 0): [number, number][] {
  const n = Math.max(3, Math.round(sides));
  return Array.from({ length: n }, (_, i) => {
    const a = (TAU * i) / n;
    const r = regularOutline(a, sides, depth);
    return [r * Math.cos(a + turn), r * Math.sin(a + turn)];
  });
}

// ---- Cratarr: the mark ---------------------------------------------------------------------------------------------

/** Cratarr's mark (lib/Mark.svelte, tools/brand/record.py's `full`): the record's rings (inner and outer radius, of its
 *  whole) outside its slice, the sixth about the right, and in the slice the thin rings that are gaps elsewhere.
 *  `share`: the slice's share of the mark's area. Its fill sends t into the slice or the rest as their areas share, and u
 *  into their rings by area, so both are filled evenly. */
export const MARK = (() => {
  const of = (rings: number[][]) => rings.map(([a, b]) => [a / 24, b / 24] as [number, number]);
  const rings = of([[0, 3], [8, 12.4], [13.6, 16.4], [17.6, 20.4], [21.6, 24]]);
  const slice = of([[12.4, 13.6], [16.4, 17.6], [20.4, 21.6]]);
  const area = (list: [number, number][]) => list.reduce((s, [a, b]) => s + b * b - a * a, 0);
  const outside = (area(rings) * 5) / 6;
  const inside = area(slice) / 6;
  return { rings, slice, share: inside / (outside + inside) };
})();

/** A point of the mark for t, u. */
export function markFill(t: number, u: number): [number, number] {
  const inSlice = t < MARK.share;
  const to = inSlice ? -Math.PI / 6 + (t / MARK.share) * (Math.PI / 3) : Math.PI / 6 + ((t - MARK.share) / (1 - MARK.share)) * ((5 * Math.PI) / 3);
  const out = ringAt(inSlice ? MARK.slice : MARK.rings, u);
  return [out * Math.cos(to), out * Math.sin(to)];
}

/** The Cratarr lens on a point (its distance `r`): into the mark. */
export function markOf(x: number, y: number, r: number): [number, number] {
  return markFill((Math.atan2(y, x) + Math.PI) / (2 * Math.PI), (r / (1 + r)) ** 2);
}

/** How far `r` is past the nearest end of `rings` (negative inside one; a ring from 0 has no inner end). */
function ringsEdge(rings: readonly (readonly [number, number])[], r: number): number {
  let best = Infinity;
  for (const [a, b] of rings) {
    if (r >= a && r <= b) return -Math.min(a === 0 ? Infinity : r - a, b - r);
    best = Math.min(best, r < a ? a - r : r - b);
  }
  return best;
}

function markEdge(x: number, y: number): number {
  const r = Math.hypot(x, y);
  const theta = Math.atan2(y, x);
  const inSlice = Math.abs(theta) < Math.PI / 6;
  const here = ringsEdge(inSlice ? MARK.slice : MARK.rings, r);
  const there = ringsEdge(inSlice ? MARK.rings : MARK.slice, r);
  // Where the slice's side and the rest differ at this distance, the slice's edge (a line from the middle) is an edge too.
  if (here < 0 === there < 0) return here;
  const across = r * Math.sin(Math.min(Math.abs(Math.abs(theta) - Math.PI / 6), Math.PI / 2));
  return here < 0 ? Math.max(here, -across) : Math.min(here, across);
}

// ---- The symbols ---------------------------------------------------------------------------------------------------

function box(x: number, y: number, bx: number, by: number): number {
  const dx = Math.abs(x) - bx;
  const dy = Math.abs(y) - by;
  return Math.hypot(Math.max(dx, 0), Math.max(dy, 0)) + Math.min(Math.max(dx, dy), 0);
}

function triangle(px: number, py: number, p: readonly [number, number][]): number {
  const [[x0, y0], [x1, y1], [x2, y2]] = p;
  const e = [
    [x1 - x0, y1 - y0],
    [x2 - x1, y2 - y1],
    [x0 - x2, y0 - y2],
  ];
  const v = [
    [px - x0, py - y0],
    [px - x1, py - y1],
    [px - x2, py - y2],
  ];
  const s = Math.sign(e[0][0] * e[2][1] - e[0][1] * e[2][0]);
  let dd = Infinity;
  let side = Infinity;
  for (let i = 0; i < 3; i++) {
    const [ex, ey] = e[i];
    const [vx, vy] = v[i];
    const k = clamp01((vx * ex + vy * ey) / (ex * ex + ey * ey));
    const qx = vx - ex * k;
    const qy = vy - ey * k;
    dd = Math.min(dd, qx * qx + qy * qy);
    side = Math.min(side, s * (vx * ey - vy * ex));
  }
  return -Math.sqrt(dd) * Math.sign(side);
}

const ARROW_HEAD: [number, number][] = [
  [1, 0],
  [0.05, 0.6],
  [0.05, -0.6],
];

/** A drop: a round bottom drawn up to a point (an uneven capsule, its top radius 0). */
function drop(x: number, y: number): number {
  const r1 = 0.58;
  const h = 1.4;
  const px = Math.abs(x);
  const py = y + 0.4;
  const b = r1 / h;
  const a = Math.sqrt(1 - b * b);
  const k = -b * px + a * py;
  if (k < 0) return Math.hypot(px, py) - r1;
  if (k > a * h) return Math.hypot(px, py - h);
  return a * px + b * py - r1;
}

/** A heart (Quilez's): its tip at the bottom, centred so its tip and its lobes touch the unit circle. */
const HEART_MIDDLE = 0.5 / (1.5 - Math.SQRT1_2);
function heart(x: number, y: number): number {
  const k = HEART_MIDDLE;
  const px = Math.abs(x * k);
  const py = y * k + k;
  const d2 = (ax: number, ay: number) => ax * ax + ay * ay;
  if (px + py > 1) return (Math.sqrt(d2(px - 0.25, py - 0.75)) - Math.SQRT2 / 4) / k;
  const m = 0.5 * Math.max(px + py, 0);
  return (Math.sqrt(Math.min(d2(px, py - 1), d2(px - m, py - m))) * Math.sign(px - py)) / k;
}

const SYMBOLS: Partial<Record<Irregular, (x: number, y: number) => number>> = {
  heart,
  cratarr: markEdge,
  ring: (x, y) => {
    const r = Math.hypot(x, y);
    return Math.max(r - 1, 0.6 - r);
  },
  cross: (x, y) => Math.min(box(x, y, 0.95, 0.28), box(x, y, 0.28, 0.95)),
  arrow: (x, y) => Math.min(box(x + 0.4, y, 0.55, 0.2), triangle(x, y, ARROW_HEAD)),
  crescent: (x, y) => Math.max(Math.hypot(x, y) - 1, 0.82 - Math.hypot(x - 0.42, y)),
  drop,
};

// ---- The warps' forms ----------------------------------------------------------------------------------------------

/** The side of a shape's grid of distances (the warps' forms are made on one; every shape is drawn from one). */
export const SDF_SIZE = 128;
/** Points of the disc each warp's form is made from. */
const FORM_POINTS = 160_000;
/** The share of them its form keeps (the nearest its middle: an unbounded warp, such as Spherical, has a far tail). */
const FORM_KEEP = 0.95;

function seeded(name: string): () => number {
  let s = 2166136261;
  for (const ch of name) s = Math.imul(s ^ ch.charCodeAt(0), 16777619);
  // mulberry32
  return () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Squared distances to the nearest set cell (Felzenszwalb and Huttenlocher's exact transform), a row-major grid. */
function distances(set: Uint8Array, n: number): Float64Array {
  const BIG = 1e20;
  const out = new Float64Array(n * n);
  for (let i = 0; i < n * n; i++) out[i] = set[i] ? 0 : BIG;
  const f = new Float64Array(n);
  const d = new Float64Array(n);
  const v = new Int32Array(n);
  const z = new Float64Array(n + 1);
  const pass = (get: (i: number) => number, put: (i: number, x: number) => void) => {
    for (let i = 0; i < n; i++) f[i] = get(i);
    let k = 0;
    v[0] = 0;
    z[0] = -Infinity;
    z[1] = Infinity;
    for (let q = 1; q < n; q++) {
      let s = (f[q] + q * q - (f[v[k]] + v[k] * v[k])) / (2 * q - 2 * v[k]);
      while (s <= z[k]) {
        k--;
        s = (f[q] + q * q - (f[v[k]] + v[k] * v[k])) / (2 * q - 2 * v[k]);
      }
      k++;
      v[k] = q;
      z[k] = s;
      z[k + 1] = Infinity;
    }
    k = 0;
    for (let q = 0; q < n; q++) {
      while (z[k + 1] < q) k++;
      d[q] = (q - v[k]) * (q - v[k]) + f[v[k]];
    }
    for (let i = 0; i < n; i++) put(i, d[i]);
  };
  for (let x = 0; x < n; x++) pass((i) => out[i * n + x], (i, val) => (out[i * n + x] = val));
  for (let y = 0; y < n; y++) pass((i) => out[y * n + i], (i, val) => (out[y * n + i] = val));
  return out;
}

/** The signed distances (units) of a grid of cells inside (1) or not, each cell's middle. */
function signed(inside: Uint8Array, n: number): Float32Array {
  const toIn = distances(inside, n);
  const toOut = distances(inside.map((v) => (v ? 0 : 1)), n);
  const unit = 2 / n;
  return Float32Array.from(inside, (v, i) => (v ? -(Math.sqrt(toOut[i]) - 0.5) : Math.sqrt(toIn[i]) - 0.5) * unit);
}

/** A warp's form: the disc's points through it, the nearest `FORM_KEEP` of them about their middle kept, centred and
 *  scaled to fit the unit circle, laid on the grid (a cell its own if a point landed by it), as signed distances. */
function warpForm(name: keyof typeof WARPS): Float32Array {
  const rnd = seeded(name);
  const xs = new Float64Array(FORM_POINTS);
  const ys = new Float64Array(FORM_POINTS);
  let m = 0;
  for (let i = 0; i < FORM_POINTS; i++) {
    const rr = Math.sqrt(rnd());
    const a = TAU * rnd();
    const x = rr * Math.cos(a);
    const y = rr * Math.sin(a);
    const r2 = x * x + y * y + 1e-12;
    const w = warpOf(WARPS[name], x, y, r2, Math.sqrt(r2), Math.atan2(x, y), rnd);
    if (!w || !Number.isFinite(w[0]) || !Number.isFinite(w[1])) continue;
    xs[m] = w[0];
    ys[m] = w[1];
    m++;
  }
  const median = (a: Float64Array) => a.slice(0, m).sort()[m >> 1];
  const mx = median(xs);
  const my = median(ys);
  const far = new Float64Array(m);
  for (let i = 0; i < m; i++) far[i] = Math.hypot(xs[i] - mx, ys[i] - my);
  const cut = far.slice().sort()[Math.floor(m * FORM_KEEP)];
  let x0 = Infinity;
  let x1 = -Infinity;
  let y0 = Infinity;
  let y1 = -Infinity;
  for (let i = 0; i < m; i++) {
    if (far[i] > cut) continue;
    x0 = Math.min(x0, xs[i]);
    x1 = Math.max(x1, xs[i]);
    y0 = Math.min(y0, ys[i]);
    y1 = Math.max(y1, ys[i]);
  }
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  let most = 0;
  for (let i = 0; i < m; i++) if (far[i] <= cut) most = Math.max(most, Math.hypot(xs[i] - cx, ys[i] - cy));
  const n = SDF_SIZE;
  const hit = new Uint8Array(n * n);
  for (let i = 0; i < m; i++) {
    if (far[i] > cut) continue;
    const gx = Math.min(n - 1, Math.max(0, Math.floor((((xs[i] - cx) / most + 1) / 2) * n)));
    const gy = Math.min(n - 1, Math.max(0, Math.floor((((ys[i] - cy) / most + 1) / 2) * n)));
    hit[gy * n + gx] = 1;
  }
  // Closed (grown a cell, then shrunk one), so the gaps between landed points are not holes.
  const grow = (src: Uint8Array, all: boolean) =>
    src.map((_, i) => {
      const x = i % n;
      const y = (i / n) | 0;
      for (let dy = -1; dy <= 1; dy++)
        for (let dx = -1; dx <= 1; dx++) {
          const xx = x + dx;
          const yy = y + dy;
          const v = xx >= 0 && yy >= 0 && xx < n && yy < n ? src[yy * n + xx] : 0;
          if (all ? !v : v) return all ? 0 : 1;
        }
      return all ? 1 : 0;
    });
  return signed(filled(grow(grow(hit, false), true), n), n);
}

/** Holes too small to be the form's own (a few cells no point happened to land in) filled. */
function filled(inside: Uint8Array, n: number, smallest = 12): Uint8Array {
  const out = inside.slice();
  const seen = new Uint8Array(n * n);
  for (let start = 0; start < n * n; start++) {
    if (inside[start] || seen[start]) continue;
    const hole: number[] = [];
    let edge = false;
    const stack = [start];
    seen[start] = 1;
    while (stack.length) {
      const i = stack.pop()!;
      hole.push(i);
      const x = i % n;
      const y = (i / n) | 0;
      if (x === 0 || y === 0 || x === n - 1 || y === n - 1) edge = true;
      for (const j of [x > 0 ? i - 1 : -1, x < n - 1 ? i + 1 : -1, y > 0 ? i - n : -1, y < n - 1 ? i + n : -1])
        if (j >= 0 && !inside[j] && !seen[j]) {
          seen[j] = 1;
          stack.push(j);
        }
    }
    if (!edge && hole.length < smallest) for (const i of hole) out[i] = 1;
  }
  return out;
}

const grids = new Map<Irregular, Float32Array>();

/** A shape's grid of signed distances (units), `SDF_SIZE` square over −1..1 each way, row-major from the bottom: what
 *  Irregular shapes draws from. Made once a shape (a warp's form takes a moment). */
export function sdfGrid(name: Irregular): Float32Array {
  let g = grids.get(name);
  if (g) return g;
  const symbol = SYMBOLS[name];
  const n = SDF_SIZE;
  if (symbol) g = Float32Array.from({ length: n * n }, (_, i) => symbol(-1 + ((i % n) + 0.5) * (2 / n), -1 + (((i / n) | 0) + 0.5) * (2 / n)));
  else g = warpForm(name as keyof typeof WARPS);
  grids.set(name, g);
  return g;
}

/** A grid's distance at (x, y), between its cells' middles; past the grid, the distance on to it added. */
function sample(g: Float32Array, x: number, y: number): number {
  const n = SDF_SIZE;
  const cx = Math.min(1, Math.max(-1, x));
  const cy = Math.min(1, Math.max(-1, y));
  const fx = Math.min(n - 1, Math.max(0, ((cx + 1) / 2) * n - 0.5));
  const fy = Math.min(n - 1, Math.max(0, ((cy + 1) / 2) * n - 0.5));
  const i = Math.min(n - 2, Math.floor(fx));
  const j = Math.min(n - 2, Math.floor(fy));
  const a = fx - i;
  const b = fy - j;
  const v = (1 - b) * ((1 - a) * g[j * n + i] + a * g[j * n + i + 1]) + b * ((1 - a) * g[(j + 1) * n + i] + a * g[(j + 1) * n + i + 1]);
  return v + Math.hypot(x - cx, y - cy);
}

/** An irregular shape's edge at (x, y). */
export function irregularEdge(name: Irregular, x: number, y: number): number {
  const symbol = SYMBOLS[name];
  return symbol ? symbol(x, y) : sample(sdfGrid(name), x, y);
}

// ---- Even fills: a shape by angle and distance -----------------------------------------------------------------------

/** Angles a fill's table holds (each a thin wedge), and steps along each wedge's middle it is read at. */
export const FILL_ANGLES = 512;
const FILL_STEPS = 512;
const FILL_REACH = 1.05;

/** A shape as Lens fills it, the general form of the mark's: for each wedge from −π round, the distances inside along
 *  its middle (rings), and the area up to and with it. */
export type FillTable = { rings: [number, number][][]; through: Float64Array; total: number };

const tables = new Map<Irregular, FillTable>();

export function fillTable(name: Irregular): FillTable {
  let t = tables.get(name);
  if (t) return t;
  const rings: [number, number][][] = [];
  const through = new Float64Array(FILL_ANGLES);
  let total = 0;
  for (let i = 0; i < FILL_ANGLES; i++) {
    const theta = -Math.PI + ((i + 0.5) * TAU) / FILL_ANGLES;
    const c = Math.cos(theta);
    const s = Math.sin(theta);
    const at = (r: number) => irregularEdge(name, r * c, r * s) < 0;
    const list: [number, number][] = [];
    let start = at(0) ? 0 : -1;
    let prev = start === 0;
    let prevR = 0;
    for (let k = 1; k <= FILL_STEPS; k++) {
      const r = (k * FILL_REACH) / FILL_STEPS;
      const now = at(r);
      if (now !== prev) {
        // The crossing, to a millionth.
        let lo = prevR;
        let hi = r;
        for (let it = 0; it < 24; it++) {
          const mid = (lo + hi) / 2;
          if (at(mid) === prev) lo = mid;
          else hi = mid;
        }
        if (now) start = hi;
        else list.push([start, lo]);
        prev = now;
      }
      prevR = r;
    }
    if (prev) list.push([start, FILL_REACH]);
    rings.push(list);
    total += list.reduce((sum, [a, b]) => sum + b * b - a * a, 0);
    through[i] = total;
  }
  t = { rings, through, total };
  tables.set(name, t);
  return t;
}

/** A point inside an irregular shape for t, u, evenly: t the wedge by the areas' share (and across it), u the distance
 *  into its rings by area. Cratarr's the mark's own (`markFill`). */
export function irregularFill(name: Irregular, t: number, u: number): [number, number] {
  if (name === 'cratarr') return markFill(t, u);
  const { rings, through, total } = fillTable(name);
  const x = clamp01(t) * total;
  let lo = 0;
  let hi = FILL_ANGLES - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (through[mid] <= x) lo = mid + 1;
    else hi = mid;
  }
  const before = lo > 0 ? through[lo - 1] : 0;
  const area = through[lo] - before;
  const across = area > 0 ? (x - before) / area : 0.5;
  const theta = -Math.PI + ((lo + across) * TAU) / FILL_ANGLES;
  const r = ringAt(rings[lo], u);
  return [r * Math.cos(theta), r * Math.sin(theta)];
}

/** Rings a wedge holds in the shaders' copy of a table (two a texel, two texels). */
export const TABLE_RINGS = 4;

/** Tables for the shader (nodes/points.ts): for each shape three rows of `FILL_ANGLES` RGBA texels, the first the area
 *  through each wedge (a share of the whole) in red, the next two its first four rings (inner, outer; none 0, 0). */
export function fillTexture(names: readonly Irregular[]): Float32Array {
  const out = new Float32Array(names.length * 3 * FILL_ANGLES * 4);
  names.forEach((name, k) => {
    const { rings, through, total } = fillTable(name);
    for (let i = 0; i < FILL_ANGLES; i++) {
      out[((k * 3) * FILL_ANGLES + i) * 4] = total > 0 ? through[i] / total : 0;
      rings[i].slice(0, TABLE_RINGS).forEach(([a, b], j) => {
        const row = k * 3 + 1 + (j >> 1);
        out[(row * FILL_ANGLES + i) * 4 + (j & 1) * 2] = a;
        out[(row * FILL_ANGLES + i) * 4 + (j & 1) * 2 + 1] = b;
      });
    }
  });
  return out;
}
