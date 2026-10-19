// The picture loop's Fluid (fluid-loop.ts, decision 0056): neutral at Amount and Stir 0, kicked by each band's hits as
// its Hits signal rises, stirred by the levels, pushed by the beats.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { FLUID_LOOP, newLoopStir, stirInputOf } from '../src/lib/visuals/fluid-loop.ts';
import { stir } from '../src/lib/visuals/stirring.ts';
import type { SignalRef } from '../src/lib/visuals/patch.ts';

const knobs = (k: Record<string, number> = {}) => ({ ...Object.fromEntries(FLUID_LOOP.knobs.map((x) => [x.id, x.default])), ...k });
const signals = (v: Record<string, number>) => (ref: SignalRef) => v[`${ref.signal}${ref.band ?? ''}`] ?? 0;

test('the knobs are continuous, each neutral within its range, Amount neutral at 0', () => {
  for (const k of FLUID_LOOP.knobs) {
    assert.ok(k.min <= k.neutral && k.neutral <= k.max && k.min <= k.default && k.default <= k.max, k.id);
    assert.equal(k.kind, 'level');
  }
  assert.equal(FLUID_LOOP.knobs.find((k) => k.id === 'amount')!.neutral, 0);
  assert.equal(FLUID_LOOP.place, 'loop');
});

test('Stir at 0: no forces, whatever the music', () => {
  const s = newLoopStir();
  for (let i = 0; i < 50; i++) {
    const f = { dt: 1 / 60, time: i / 60, beat: { index: i, since: 0, length: 0.5, strength: 1 }, signals: signals({ hits0: i % 2, level0: 1 }) };
    assert.equal(stir(s.stirring, stirInputOf(s, f, knobs({ stir: 0 }))).forces.length, 0);
  }
});

test("a band's Hits rising is a kick at its place; its level stirs steadily", () => {
  const s = newLoopStir();
  const at = (i: number, hits: number, level: number) => ({ dt: 1 / 60, time: i / 60, beat: null, signals: signals({ hits2: hits, level2: level }) });
  // First seen: nothing kicked for what came before.
  stir(s.stirring, stirInputOf(s, at(0, 0, 0), knobs()));
  stir(s.stirring, stirInputOf(s, at(1, 0, 0), knobs()));
  const kicked = stir(s.stirring, stirInputOf(s, at(2, 0.9, 0), knobs())).forces;
  const kicked2 = stir(s.stirring, stirInputOf(s, at(3, 0.2, 0), knobs()));
  const hit = stir(s.stirring, stirInputOf(s, at(4, 1, 0), knobs())).forces;
  assert.equal(kicked.length, 0, 'the first hit seen is only noted');
  assert.equal(kicked2.forces.length, 0);
  assert.equal(hit.length, 1, 'the next one kicks');
  // At band 2's place (60° and a half round the middle, a quarter out), pushing outward.
  const [x, y] = [0.5 + 0.25 * Math.cos((5 * Math.PI) / 6), 0.5 + 0.25 * Math.sin((5 * Math.PI) / 6)];
  assert.ok(Math.hypot(hit[0].x - x, hit[0].y - y) < 1e-9 && hit[0].strength > 0);
  const steady = stir(newLoopStir().stirring, stirInputOf(newLoopStir(), at(0, 0, 0.8), knobs())).forces;
  assert.ok(steady.length === 1 && steady[0].strength > 0, 'its level stirs');
});

test('signals out of range or not numbers are taken as 0..1', () => {
  const s = newLoopStir();
  const input = stirInputOf(s, { dt: 1 / 60, time: 0, beat: null, signals: () => Number.NaN }, knobs());
  assert.ok(input.level.every((v) => v === 0));
  const big = stirInputOf(newLoopStir(), { dt: 1 / 60, time: 0, beat: null, signals: () => 7 }, knobs());
  assert.ok(big.level.every((v) => v === 1) && big.loudness === 1);
});
