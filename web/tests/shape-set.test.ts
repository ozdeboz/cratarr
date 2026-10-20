// One set of shapes (shape-set.ts, decision 0057 §4g): the list as decided; every shape's fill inside its own edge and
// even; the polygon and the Cratarr mark exactly as before; every lens a track can have draws its points exactly as
// before the set (its warps and the mark moved to warps.ts and shape-set.ts), but Heart, now the real heart: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const S = await import('../src/lib/visuals/shape-set.ts');
const { morphEdge } = await import('../src/lib/visuals/sources/geometry.ts');
const { apply, flameOf, LENS_SHAPE_VARIATIONS, LENSES, VARIATIONS, randomFrom } = await import('../src/lib/visuals/flame.ts');
const { WARPS } = await import('../src/lib/visuals/warps.ts');

// A look to make flames from.
const preset = {
  form: 0,
  detail: 0.5,
  twist: 0.7,
  warp: 0.8,
  symmetry: 1,
  mirror: 0,
  final: 0,
  family: 0,
  hue: 0.2,
  lightness: 0.5,
  vividness: 0.5,
  spread: 1,
  purity: 0.75,
  fx1: 0,
  fx1Amount: 0,
  fx2: 0,
  fx2Amount: 0,
  fx3: 0,
  fx3Amount: 0,
  fxLoop: 0,
  fxLoopAmount: 0,
  fx1K1: 0.5,
  fx1K2: 0.5,
  fx2K1: 0.5,
  fx2K2: 0.5,
  fx3K1: 0.5,
  fx3K2: 0.5,
  fxLoopK1: 0.5,
  fxLoopK2: 0.5,
  facets: 0,
  lattice: 0,
  lensSides: 5,
  lensDepth: 0.5,
};

type Shape = { name: string; edge: (x: number, y: number) => number; fill: (t: number, u: number) => [number, number] };
const regular = (sides: number, depth: number): Shape => ({
  name: `${depth ? 'star' : 'polygon'} ${sides}/${depth}`,
  edge: (x, y) => S.regularEdge(Math.hypot(x, y), Math.atan2(y, x), sides, depth),
  fill: (t, u) => S.regularFill(t, u, sides, depth),
});
const SHAPES: Shape[] = [
  ...[3, 4, 5, 8, 48].flatMap((n) => [regular(n, 0), regular(n, 0.5), regular(n, 1)]),
  ...S.IRREGULAR.map((name) => ({ name, edge: (x: number, y: number) => S.irregularEdge(name, x, y), fill: (t: number, u: number) => S.irregularFill(name, t, u) })),
];

test('the list: two regular, nine irregular (only distinct forms), in the decided order', () => {
  assert.deepEqual(S.REGULAR, ['polygon', 'star']);
  assert.deepEqual(S.IRREGULAR, ['heart', 'cratarr', 'ring', 'cross', 'arrow', 'crescent', 'drop', 'polar', 'handkerchief']);
  // The warps' forms are the lens's warps, each by its number; the parts keep flam3's heart.
  for (const name of S.IRREGULAR.slice(7)) assert.equal(WARPS[name as keyof typeof WARPS], VARIATIONS[name as keyof typeof VARIATIONS]);
  assert.equal(WARPS.heart, VARIATIONS.heart);
  // A real heart: its tip at the bottom, its notch at the top, its lobes either side; it fits the unit circle.
  assert.ok(S.irregularEdge('heart', 0, -0.95) < 0 && S.irregularEdge('heart', 0, -1.05) > 0, 'its tip');
  assert.ok(S.irregularEdge('heart', 0, 0.7) > 0 && S.irregularEdge('heart', 0, 0.5) < 0, 'its notch');
  assert.ok(S.irregularEdge('heart', -0.4, 0.5) < 0 && S.irregularEdge('heart', 0.4, 0.5) < 0, 'its lobes');
  assert.ok(Math.abs(S.irregularEdge('heart', 0.3, 0) - S.irregularEdge('heart', -0.3, 0)) < 1e-12, 'mirrored');
  // The lens's shapes by their own numbers, in the set's order; flam3's Cross stays its own.
  assert.deepEqual(Object.keys(LENS_SHAPE_VARIATIONS), [...S.LENS_SHAPES]);
  assert.equal(VARIATIONS.cross, 48);
});

const grid = (f: (t: number, u: number) => [number, number], nt = 300, nu = 60) => {
  const out: [number, number][] = [];
  for (let a = 0; a < nt; a++) for (let b = 0; b < nu; b++) out.push(f((a + 0.5) / nt, (b + 0.5) / nu));
  return out;
};

test('every fill lands inside its own shape, and fits the unit circle', () => {
  for (const s of SHAPES) {
    const pts = grid(s.fill);
    const outside = pts.filter(([x, y]) => s.edge(x, y) > 0.01).length;
    assert.ok(outside / pts.length <= 0.01, `${s.name}: ${outside} of ${pts.length} outside`);
    assert.ok(pts.every(([x, y]) => Math.hypot(x, y) <= 1.06), `${s.name}: past the unit circle`);
  }
});

test('every fill is even: each quarter of the plane gets the share of points its share of the area is', () => {
  for (const s of SHAPES) {
    const area = [0, 0, 0, 0];
    const n = 200;
    for (let j = 0; j < n; j++)
      for (let i = 0; i < n; i++) {
        const x = -1.05 + ((i + 0.5) * 2.1) / n;
        const y = -1.05 + ((j + 0.5) * 2.1) / n;
        if (s.edge(x, y) < 0) area[(x < 0 ? 1 : 0) + (y < 0 ? 2 : 0)]++;
      }
    const got = [0, 0, 0, 0];
    const pts = grid(s.fill);
    for (const [x, y] of pts) got[(x < 0 ? 1 : 0) + (y < 0 ? 2 : 0)]++;
    const whole = area.reduce((a, b) => a + b, 0);
    for (let q = 0; q < 4; q++) assert.ok(Math.abs(got[q] / pts.length - area[q] / whole) < 0.02, `${s.name} quarter ${q}: ${(got[q] / pts.length).toFixed(3)} of the points, ${(area[q] / whole).toFixed(3)} of the area`);
  }
});

test('the polygon is geometry.ts’s, exactly; a star at depth 0 is the polygon', () => {
  for (const sides of [3, 4.5, 7, 12.25])
    for (let k = 0; k < 50; k++) {
      const r = k / 40;
      const th = k * 0.37 - 3;
      assert.equal(S.regularEdge(r, th, sides, 0), morphEdge(r, th, sides, 1));
      assert.ok(Math.abs(S.regularOutline(th, Math.round(sides), 1e-9) - S.regularOutline(th, Math.round(sides), 0)) < 1e-6);
    }
  assert.equal(S.regularCorners(5, 0).length, 5);
  for (const [x, y] of S.regularCorners(6, 0.7, 0.3)) assert.ok(Math.abs(Math.hypot(x, y) - 1) < 1e-9, 'corners at radius 1');
});

// ---- Every lens as before -------------------------------------------------------------------------------------------

// The mark as flame.ts had it before the set (2026-10-20).
const OLD_MARK = (() => {
  const of = (rings: number[][]) => rings.map(([a, b]) => [a / 24, b / 24] as [number, number]);
  const rings = of([[0, 3], [8, 12.4], [13.6, 16.4], [17.6, 20.4], [21.6, 24]]);
  const slice = of([[12.4, 13.6], [16.4, 17.6], [20.4, 21.6]]);
  const area = (list: [number, number][]) => list.reduce((s, [a, b]) => s + b * b - a * a, 0);
  return { rings, slice, share: (area(slice) / 6) / ((area(rings) * 5) / 6 + area(slice) / 6) };
})();
function oldRingAt(rings: [number, number][], u: number): number {
  const total = rings.reduce((s, [a, b]) => s + b * b - a * a, 0);
  let left = Math.min(1, Math.max(0, u)) * total;
  for (const [a, b] of rings) {
    const area = b * b - a * a;
    if (left < area) return Math.sqrt(a * a + left);
    left -= area;
  }
  return rings[rings.length - 1][1];
}
function oldMarkOf(x: number, y: number, r: number): [number, number] {
  const t = (Math.atan2(y, x) + Math.PI) / (2 * Math.PI);
  const u = (r / (1 + r)) ** 2;
  const inSlice = t < OLD_MARK.share;
  const to = inSlice ? -Math.PI / 6 + (t / OLD_MARK.share) * (Math.PI / 3) : Math.PI / 6 + ((t - OLD_MARK.share) / (1 - OLD_MARK.share)) * ((5 * Math.PI) / 3);
  const out = oldRingAt(inSlice ? OLD_MARK.slice : OLD_MARK.rings, u);
  return [out * Math.cos(to), out * Math.sin(to)];
}

// A final transform as flame.ts's `apply` drew it before the set: its warps' formulas as they were written there.
function oldApply(t: { affine: number[]; post?: number[]; variations: Map<number, number> }, px: number, py: number, rnd: () => number): [number, number] {
  const [a, b, c, d, e, f] = t.affine;
  const x = a * px + b * py + c;
  const y = d * px + e * py + f;
  let ox = 0;
  let oy = 0;
  const r2 = x * x + y * y + 1e-12;
  const r = Math.sqrt(r2);
  const theta = Math.atan2(x, y);
  for (const [v, w] of t.variations) {
    let vx = x;
    let vy = y;
    switch (v) {
      case 1: vx = Math.sin(x); vy = Math.sin(y); break;
      case 2: vx = x / r2; vy = y / r2; break;
      case 3: vx = x * Math.sin(r2) - y * Math.cos(r2); vy = x * Math.cos(r2) + y * Math.sin(r2); break;
      case 4: vx = ((x - y) * (x + y)) / r; vy = (2 * x * y) / r; break;
      case 5: vx = theta / Math.PI; vy = r - 1; break;
      case 6: vx = r * Math.sin(theta + r); vy = r * Math.cos(theta - r); break;
      case 7: vx = r * Math.sin(theta * r); vy = -r * Math.cos(theta * r); break;
      case 99: [vx, vy] = oldMarkOf(x, y, r); break;
      case 8: vx = (theta / Math.PI) * Math.sin(Math.PI * r); vy = (theta / Math.PI) * Math.cos(Math.PI * r); break;
      case 13: {
        const om = rnd() < 0.5 ? 0 : Math.PI;
        const sr = Math.sqrt(r);
        vx = sr * Math.cos(theta / 2 + om);
        vy = sr * Math.sin(theta / 2 + om);
        break;
      }
      case 16: vx = (2 / (r + 1)) * y; vy = (2 / (r + 1)) * x; break;
      case 27: vx = (2 / (r + 1)) * x; vy = (2 / (r + 1)) * y; break;
      case 28: vx = (4 / (r2 + 4)) * x; vy = (4 / (r2 + 4)) * y; break;
    }
    ox += w * vx;
    oy += w * vy;
  }
  if (t.post) return [t.post[0] * ox + t.post[1] * oy + t.post[2], t.post[3] * ox + t.post[4] * oy + t.post[5]];
  return [ox, oy];
}

test('every lens a track could have before draws exactly as before but Heart (the real heart now); the set’s new ones after them', () => {
  assert.deepEqual(LENSES.slice(0, 13), ['Spherical', 'Swirl', 'Julia', 'Fisheye', 'Eyefish', 'Bubble', 'Polar', 'Disc', 'Sinusoidal', 'Horseshoe', 'Handkerchief', 'Heart', 'Cratarr']);
  assert.deepEqual(LENSES.slice(13), ['Polygon', 'Star', 'Ring', 'Cross', 'Arrow', 'Crescent', 'Drop']);
  assert.deepEqual(flameOf({ ...preset, final: 12 }, false).final!.variations, new Map([[LENS_SHAPE_VARIATIONS.heart, 1]]));
  for (const lens of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13])
    for (const form of [0, 17, 404]) {
      const final = flameOf({ ...preset, form, final: lens }, false).final!;
      assert.equal(final.shape, undefined);
      const a = randomFrom(lens * 1000 + form);
      const b = randomFrom(lens * 1000 + form);
      const p = randomFrom(7);
      for (let k = 0; k < 300; k++) {
        const x = (p() * 2 - 1) * 1.5;
        const y = (p() * 2 - 1) * 1.5;
        assert.deepEqual(apply(final, x, y, a), oldApply(final, x, y, b), `lens ${LENSES[lens - 1]}, form ${form}, point ${k}`);
      }
    }
  // The mark itself: markOf as it was.
  for (let k = 0; k < 500; k++) {
    const x = Math.sin(k * 1.7) * 2;
    const y = Math.cos(k * 0.9) * 2;
    const r = Math.hypot(x, y);
    assert.deepEqual(S.markOf(x, y, r), oldMarkOf(x, y, r));
    const [t, u] = S.lensAt(x, y, r);
    assert.deepEqual(S.irregularFill('cratarr', t, u), oldMarkOf(x, y, r));
  }
});

test('a lens’s shape: its points sent into the shape, Polygon and Star by its sides and depth', () => {
  for (const [i, name] of S.LENS_SHAPES.entries()) {
    // Heart's at its place of old; the rest after Cratarr.
    const lens = name === 'heart' ? 12 : 14 + i;
    assert.equal(LENSES[lens - 1].toLowerCase(), name);
    const final = flameOf({ ...preset, final: lens, lensSides: 6, lensDepth: 0.6 }, false).final!;
    assert.deepEqual(final.variations, new Map([[LENS_SHAPE_VARIATIONS[name], 1]]));
    const rnd = randomFrom(3);
    const edge = name === 'polygon' ? regular(6, 0).edge : name === 'star' ? regular(6, 0.6).edge : (x: number, y: number) => S.irregularEdge(name, x, y);
    let outside = 0;
    for (let k = 0; k < 400; k++) {
      // Wherever a point is, the lens sends it into the shape.
      const [x, y] = S.lensInto(name, rnd() * 2 - 1, rnd() * 2 - 1, rnd(), { sides: 6, depth: 0.6 });
      if (edge(x, y) > 0.01) outside++;
    }
    assert.ok(outside <= 4, `${name}: ${outside} outside`);
    assert.doesNotThrow(() => apply(final, 0.3, -0.2, rnd));
  }
});

test('the shader’s copy of the Lens’s symbols’ tables: the share through each wedge, and every ring of it', () => {
  const names = S.LENS_SHAPES.slice(2) as unknown as (typeof S.IRREGULAR)[number][];
  const tex = S.fillTexture(names);
  names.forEach((name, k) => {
    const { rings, through, total } = S.fillTable(name);
    for (let i = 0; i < S.FILL_ANGLES; i++) {
      assert.ok(rings[i].length <= S.TABLE_RINGS, `${name}: wedge ${i} has ${rings[i].length} rings`);
      assert.ok(Math.abs(tex[(k * 3 * S.FILL_ANGLES + i) * 4] - through[i] / total) < 1e-6);
      rings[i].forEach(([a, b], j) => {
        const at = ((k * 3 + 1 + (j >> 1)) * S.FILL_ANGLES + i) * 4 + (j & 1) * 2;
        assert.ok(Math.abs(tex[at] - a) < 1e-6 && Math.abs(tex[at + 1] - b) < 1e-6);
      });
    }
  });
});
