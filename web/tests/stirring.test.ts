// The fluid's stirring through its routes (stirring.ts, routes.ts heightOn): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { newStirring, stir, type Gain, type StirInput } from '../src/lib/visuals/stirring.ts';
import { compile, defaultRoutes, heightOn, routeKey } from '../src/lib/visuals/routes.ts';
import { heightAt, tableOf, type Curve } from '../src/lib/visuals/curves.ts';

const BANDS = 6;
const close = (a: number, b: number, eps = 1e-9) => Math.abs(a - b) < eps;

/** Two seconds of music at 120 BPM: a beat every half second, each band hitting in turn, a downbeat every fourth beat,
 *  a new section at one second; the fluid on. */
function* music(): Generator<StirInput> {
  for (let f = 0; f < 120; f++) {
    const t = f / 60;
    const beatIndex = Math.floor(t * 2);
    const since = t * 2 - beatIndex;
    yield {
      t,
      dt: 1 / 60,
      amount: 0.5,
      parts: Array.from({ length: BANDS }, (_, b) => [0.3 + 0.08 * b, 0.6 - 0.05 * b] as [number, number]),
      turns: Array.from({ length: BANDS }, (_, b) => (b % 2 ? 1 : -1)),
      lights: Array(BANDS).fill(0),
      level: Array.from({ length: BANDS }, (_, b) => 0.2 + 0.1 * b),
      continuity: [0, 0.25, 0.5, 0.5, 0.75, 1],
      hits: Array.from({ length: BANDS }, (_, b) => ({ index: Math.floor(t * 3 + b / 6), strength: 0.2 + 0.13 * b, nextIn: null })),
      beat: { index: beatIndex, since: since * 0.5, length: 0.5, strength: 0.4 + 0.1 * (beatIndex % 4), nextIn: (1 - since) * 0.5 },
      downbeat: beatIndex % 4 === 0 && since < 0.2 ? 1 : 0,
      section: t >= 1 && t < 1.2 ? 1 : 0,
      loudness: 0.4 + 0.3 * Math.sin(t * 3),
      swell: 0.5 + 0.5 * Math.sin(t),
      bps: 2,
      center: [0.5, 0.5],
    };
  }
}

const run = (gain?: Gain) => {
  const s = newStirring();
  return [...music()].map((i) => stir(s, { ...i, gain }));
};

test('the default routes into Stir, Flow and Swirl stir exactly as before them', () => {
  const m = compile(defaultRoutes(BANDS), BANDS);
  const before = run();
  const now = run((signal, band, aspect, x) => heightOn(m, signal, band, aspect, x));
  let kicks = 0;
  before.forEach((b, f) => {
    const n = now[f];
    assert.ok(close(b.flow.energy, n.flow.energy) && close(b.flow.swirl, n.flow.swirl), `flow at frame ${f}`);
    assert.equal(n.forces.length, b.forces.length, `forces at frame ${f}`);
    b.forces.forEach((x, k) => assert.ok(close(x.strength, n.forces[k].strength) && close(x.dx, n.forces[k].dx), `force ${k} at frame ${f}`));
    kicks += b.forces.length;
  });
  assert.ok(kicks > 100, 'the music stirred');
  assert.ok(before.some((b) => b.flow.swirl > 2.5), 'a section whirled');
});

test('a route halved halves its pushes; without it, none; a curve shapes them', () => {
  const routes = { ...defaultRoutes(BANDS), [routeKey('hits', 0, 'stir')]: 0.5 } as Record<string, number | Curve>;
  delete routes[routeKey('beat', null, 'stir')];
  delete routes[routeKey('downbeat', null, 'stir')];
  delete routes[routeKey('section', null, 'stir')];
  for (let b = 1; b < BANDS; b++) delete routes[routeKey('hits', b, 'stir')];
  for (let b = 0; b < BANDS; b++) delete routes[routeKey('level', b, 'stir')];
  const m = compile(routes, BANDS);
  const only = run((signal, band, aspect, x) => heightOn(m, signal, band, aspect, x));
  const all = run();
  // Band 0's part (stepped: Continuity 0) is the only one kicked, at half its strength.
  const at0 = (r: ReturnType<typeof run>) => r.flatMap((o) => o.forces.filter((x) => close(x.x, 0.3) && close(x.y, 0.6)).map((x) => x.strength));
  const others = only.flatMap((o) => o.forces.filter((x) => !(close(x.x, 0.3) && close(x.y, 0.6))));
  assert.equal(others.length, 0, 'nothing else pushes');
  assert.ok(at0(only).length > 0);
  at0(only).forEach((v, k) => assert.ok(close(v, at0(all)[k] * 0.5), `kick ${k}`));
  // Swirl and Flow as their own routes say: swell's at 0, section's whirl alone.
  const calm = compile({ ...defaultRoutes(BANDS), [routeKey('swell', null, 'swirl')]: 0 }, BANDS);
  const r = run((signal, band, aspect, x) => heightOn(calm, signal, band, aspect, x));
  assert.ok(r.slice(0, 30).every((o) => close(o.flow.swirl, 1, 1e-6)), 'no swell, no section yet: no swirl');
  // A curve: Loudness → Flow rising fast then flat.
  const fast: Curve = { h1: [0.1, 0.9], h2: [0.3, 1], h: 1 };
  const shaped = compile({ ...defaultRoutes(BANDS), [routeKey('loudness', null, 'flow')]: fast }, BANDS);
  const { table } = tableOf(fast);
  const i = [...music()][10];
  const o = stir(newStirring(), { ...i, gain: (signal, band, aspect, x) => heightOn(shaped, signal, band, aspect, x) });
  const pulse = Math.exp(-i.beat!.since / Math.max(0.05, 0.3 * i.beat!.length));
  assert.ok(close(o.flow.energy, Math.min(1, heightAt(table, i.loudness) * (0.7 + 0.6 * pulse)), 1e-6));
});

test('a route read at a moment: its curve, a straight one times x, none without one, at its walk’s share', () => {
  const bent: Curve = { h1: [0.2, 0.9], h2: [0.4, 1], h: 0.8, walks: true };
  const m = compile({ [routeKey('beat', null, 'stir')]: 0.6, [routeKey('level', 2, 'stir')]: bent }, BANDS);
  assert.ok(close(heightOn(m, 'beat', null, 'stir', 0.5), 0.3, 1e-6), 'kept as a 32-bit float');
  assert.equal(heightOn(m, 'section', null, 'stir', 1), 0);
  assert.equal(heightOn(m, 'level', 3, 'stir', 1), 0);
  assert.ok(close(heightOn(m, 'level', 2, 'stir', 0.5), heightAt(tableOf(bent).table, 0.5), 1e-6));
  assert.ok(close(heightOn(m, 'level', 2, 'stir', 0.5, Float32Array.from([0.7])), 0.7 * heightAt(tableOf(bent).table, 0.5), 1e-6));
});

test('the waves: as high as the Waves column gives, rolling with the track’s own time; none without it', () => {
  const [i] = [...music()];
  assert.equal(stir(newStirring(), i).flow.wave, undefined, 'no Waves column: no waves');
  const a = stir(newStirring(), { ...i, waves: 0.5, position: 30 }).flow.wave!;
  assert.equal(a.amount, 0.5);
  // (each kept within a turn: 30 s is three and three quarter waves, a quarter of the way's turn)
  assert.ok(close(a.phase, 0.75 * 2 * Math.PI) && close(a.angle, 0.25 * 2 * Math.PI), 'phase and way from the track’s time');
  // Two devices at the same place in the track wave alike, whatever their own clocks.
  const b = stir(newStirring(), { ...i, t: i.t + 1234, waves: 0.5, position: 30 }).flow.wave!;
  assert.deepEqual(a, b);
});
