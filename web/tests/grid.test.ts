// The Grid (engine/grid.ts, decision 0057 §4c): its lattices' points and their channels.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { gridOf, GOLDEN_ANGLE } = await import('../src/lib/visuals/engine/grid.ts');
const { LATTICES, MOST_POINTS } = await import('../src/lib/visuals/ports.ts');
const { seedOf } = await import('../src/lib/visuals/flame.ts');
type Grid = import('../src/lib/visuals/engine/grid.ts').Grid;

const SEED = seedOf('grid-1');
const at = (lattice: (typeof LATTICES)[number]) => LATTICES.indexOf(lattice);
const near = (a: number, b: number, eps = 1e-12) => Math.abs(a - b) <= eps;
/** Whether every point has its mirror through `f` among the points. */
const symmetric = (g: Grid, f: (p: { x: number; y: number }) => { x: number; y: number }) =>
  g.points.every((p) => {
    const q = f(p);
    return g.points.some((r) => near(r.x, q.x, 1e-9) && near(r.y, q.y, 1e-9));
  });
/** The nearest other point's distance from each point. */
const nearest = (g: Grid) => g.points.map((p, i) => Math.min(...g.points.filter((_, j) => j !== i).map((q) => Math.hypot(p.x - q.x, p.y - q.y))));

test('counts: as many points as asked, one to 64, a channel set each', () => {
  for (const lattice of LATTICES)
    for (const count of [1, 2, 7, 16, 19, 37, 50, MOST_POINTS, 200, 0.4]) {
      const g = gridOf({ lattice: at(lattice), count }, SEED);
      const want = Math.min(MOST_POINTS, Math.max(1, Math.round(count)));
      assert.equal(g.points.length, want, `${lattice} ${count}`);
      assert.equal(g.channels.length, want, `${lattice} ${count}`);
      for (const c of g.channels) for (const v of Object.values(c)) assert.ok(v >= 0 && v <= 1, `${lattice} ${count}: ${JSON.stringify(c)}`);
    }
  // Unset knobs: its defaults (16 on a square).
  assert.equal(gridOf({}, SEED).points.length, 16);
});

test('square and hexagonal are centred and symmetric; every lattice its neighbours a spacing apart', () => {
  const s = 0.2;
  for (const count of [9, 16, 36, 64]) {
    const g = gridOf({ lattice: at('square'), count, spacing: s }, SEED);
    assert.ok(symmetric(g, (p) => ({ x: -p.x, y: p.y })) && symmetric(g, (p) => ({ x: p.x, y: -p.y })), `square ${count}`);
    assert.ok(symmetric(g, (p) => ({ x: p.y, y: p.x })), `square ${count} on its diagonal`);
    assert.ok(nearest(g).every((d) => near(d, s, 1e-9)));
  }
  for (const count of [16, 36, 64]) {
    const g = gridOf({ lattice: at('hexagonal'), count, spacing: s }, SEED);
    assert.ok(symmetric(g, (p) => ({ x: -p.x, y: -p.y })), `hexagonal ${count} through its centre`);
    const sum = g.points.reduce((a, p) => ({ x: a.x + p.x, y: a.y + p.y }), { x: 0, y: 0 });
    assert.ok(near(sum.x, 0, 1e-9) && near(sum.y, 0, 1e-9), `hexagonal ${count}: centred`);
    assert.ok(nearest(g).every((d) => near(d, s, 1e-9)));
    // Six neighbours a spacing away, inside.
    const inner = g.points.filter((p) => g.points.filter((q) => near(Math.hypot(p.x - q.x, p.y - q.y), s, 1e-9)).length === 6);
    assert.ok(inner.length >= count / 4, `hexagonal ${count}: ${inner.length} with six`);
  }
  // Rings: a spacing apart along each ring, about, and between them.
  const full = gridOf({ lattice: at('rings'), count: 37, spacing: s }, SEED);
  assert.ok(nearest(full).every((d) => d >= s - 1e-9 && d <= s * 1.05), `rings: ${Math.min(...nearest(full))} .. ${Math.max(...nearest(full))}`);
  // Triangular: three neighbours a spacing away (the honeycomb's), none nearer; an end of a row may have lost its own.
  const tri = gridOf({ lattice: at('triangular'), count: 36, spacing: s }, SEED);
  const around = tri.points.map((p) => tri.points.filter((q) => near(Math.hypot(p.x - q.x, p.y - q.y), s, 1e-9)).length);
  assert.equal(Math.max(...around), 3);
  assert.ok(around.filter((k) => k === 3).length >= 12, `${around}`);
  assert.ok(nearest(tri).every((d) => d >= s - 1e-9));
  // Rings: a centre point, then six, twelve, eighteen, each ring a spacing further and evenly round.
  const rings = gridOf({ lattice: at('rings'), count: 37, spacing: s }, SEED);
  assert.ok(near(Math.hypot(rings.points[0].x, rings.points[0].y), 0));
  assert.deepEqual([1, 2, 3].map((k) => rings.points.filter((p) => near(Math.hypot(p.x, p.y), k * s, 1e-9)).length), [6, 12, 18]);
  assert.ok(symmetric(rings, (p) => ({ x: -p.x, y: -p.y })));
});

test("the spiral: each point the golden angle on from the last, its radius growing as the count's root", () => {
  const s = 0.15;
  assert.ok(near((GOLDEN_ANGLE * 180) / Math.PI, 137.50776405003785, 1e-9));
  for (const rotation of [0, 0.3]) {
    const g = gridOf({ lattice: at('spiral'), count: MOST_POINTS, spacing: s, rotation }, SEED);
    for (let i = 2; i < g.points.length; i++) {
      const a = Math.atan2(g.points[i].y, g.points[i].x) - Math.atan2(g.points[i - 1].y, g.points[i - 1].x);
      const turned = ((a % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
      assert.ok(near(turned, GOLDEN_ANGLE, 1e-9), `point ${i}: ${turned}`);
      assert.ok(near(Math.hypot(g.points[i].x, g.points[i].y), (s / Math.sqrt(Math.PI)) * Math.sqrt(i), 1e-12));
    }
  }
});

test('jitter 0 is exact, and seeded: the same every frame, its own for each module, within half a spacing', () => {
  for (const lattice of LATTICES) {
    const exact = gridOf({ lattice: at(lattice), count: 30, spacing: 0.2 }, SEED);
    assert.deepEqual(gridOf({ lattice: at(lattice), count: 30, spacing: 0.2, jitter: 0 }, seedOf('another')), exact);
    const a = gridOf({ lattice: at(lattice), count: 30, spacing: 0.2, jitter: 1 }, SEED);
    assert.deepEqual(gridOf({ lattice: at(lattice), count: 30, spacing: 0.2, jitter: 1 }, SEED), a);
    const b = gridOf({ lattice: at(lattice), count: 30, spacing: 0.2, jitter: 1 }, seedOf('grid-2'));
    assert.ok(a.points.some((p, i) => !near(p.x, b.points[i].x)));
    const moved = a.points.map((p, i) => Math.hypot(p.x - exact.points[i].x, p.y - exact.points[i].y));
    assert.ok(moved.every((d) => d <= 0.1 + 1e-12) && moved.some((d) => d > 0.02), `${lattice}: ${Math.max(...moved)}`);
  }
});

test('distance and angle are those of the points; index, row and column their places', () => {
  for (const lattice of LATTICES)
    for (const knobs of [{ count: 25 }, { count: 64, rotation: 0.125, jitter: 0.5 }]) {
      const g = gridOf({ lattice: at(lattice), spacing: 0.1, ...knobs }, SEED);
      const far = Math.max(...g.points.map((p) => Math.hypot(p.x, p.y)));
      g.points.forEach((p, i) => {
        const c = g.channels[i];
        assert.ok(near(c.distance * far, Math.hypot(p.x, p.y), 1e-12), `${lattice} ${i} distance`);
        if (Math.hypot(p.x, p.y) > 1e-9) {
          const turn = c.angle * 2 * Math.PI;
          assert.ok(near(Math.cos(turn) * Math.hypot(p.x, p.y), p.x, 1e-12) && near(Math.sin(turn) * Math.hypot(p.x, p.y), p.y, 1e-12), `${lattice} ${i} angle`);
        }
        assert.ok(near(c.index, i / (g.points.length - 1)));
      });
      assert.ok(g.channels.some((c) => c.distance === 1));
    }
  // A square of 16: four rows of four, top row first, each left to right.
  const sq = gridOf({ lattice: at('square'), count: 16, spacing: 0.2 }, SEED);
  assert.deepEqual(sq.channels.map((c) => [c.row * 3, c.column * 3].map(Math.round)), Array.from({ length: 16 }, (_, i) => [Math.floor(i / 4), i % 4]));
  assert.ok(sq.points[0].y > 0 && sq.points[0].x < 0 && sq.points[3].x > 0);
  // Rotation turns the whole, the channels' places kept.
  const turned = gridOf({ lattice: at('square'), count: 16, spacing: 0.2, rotation: 0.25 }, SEED);
  turned.points.forEach((p, i) => assert.ok(near(p.x, -sq.points[i].y, 1e-12) && near(p.y, sq.points[i].x, 1e-12)));
  assert.deepEqual(turned.channels.map((c) => [c.row, c.column]), sq.channels.map((c) => [c.row, c.column]));
});
