// What the dice make of a recipe (recipe.ts settingsOf): everything, or the movement only (the double tap), and what
// the owner pinned kept either way: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MOVEMENT, recipeFor, settingsOf } from '../src/lib/visuals/recipe.ts';
import { ASPECTS, SCALE_KEYS } from '../src/lib/visuals/routes.ts';
import type { VisualSettings } from '../src/lib/visuals/settings.svelte.ts';

const six = (v: number) => Array<number>(6).fill(v);
const base = {
  quality: 0.6,
  fps: null,
  brightness: 1,
  wild: 0.5,
  pins: [] as string[],
  routes: { 'track|-|look': -1, 'swell|-|fluid': 0.5, 'loudness|-|hue': 0.3, 'loudness|-|size': 0.9, 'level|0|shape': 0.4 },
  flash: 0.33,
  speed: 0.44,
  intensity: 0.55,
  fluid: 0.66,
  smoothing: 0.11,
  haze: 0.12,
  lighting: six(0.5),
  evolution: six(0.5),
  continuity: six(0.5),
  ...Object.fromEntries(SCALE_KEYS.map((k) => [k, 0.2])),
} as unknown as VisualSettings;
const recipe = recipeFor(12345, ASPECTS);
const columnOf = (k: string) => k.split('|')[2];
const moving = (k: string) => (MOVEMENT.columns as readonly string[]).includes(columnOf(k));

test('a full roll keeps the Track row and takes the recipe for the rest', () => {
  const all = settingsOf(recipe, base);
  assert.equal(all.routes['track|-|look'], -1);
  assert.notEqual(all.intensity, 0.55);
  assert.notEqual(all.flash, 0.33);
});

test('a movement roll changes only the movement', () => {
  const mv = settingsOf(recipe, base, 'movement');
  for (const k of ['track|-|look', 'swell|-|fluid', 'loudness|-|hue']) assert.equal(mv.routes[k], base.routes[k], k);
  for (const k of Object.keys(recipe.routes).filter(moving)) assert.equal(mv.routes[k], recipe.routes[k], k);
  for (const k of Object.keys(mv.routes).filter(moving)) assert.equal(mv.routes[k], recipe.routes[k], k);
  assert.deepEqual([mv.intensity, mv.fluid, mv.smoothing, mv.haze], [0.55, 0.66, 0.11, 0.12]);
  assert.notEqual(mv.flash, 0.33);
  assert.notEqual(mv.speed, 0.44);
  assert.notEqual(mv.lighting[0], 0.5);
  assert.notEqual(mv.evolution[0], 0.5);
  assert.notEqual(mv.continuity[0], 0.5);
});

test('what is pinned stays, in either roll', () => {
  const pinned = { ...base, pins: ['flash', 'route:loudness|-|size', 'route:level|0|shape'] } as VisualSettings;
  for (const scope of ['all', 'movement'] as const) {
    const s = settingsOf(recipe, pinned, scope);
    assert.equal(s.flash, 0.33, scope);
    assert.equal(s.routes['loudness|-|size'], 0.9, scope);
    assert.equal(s.routes['level|0|shape'], 0.4, scope);
  }
});

test('a roll leaves the fluid’s stirring (Stir, Flow, Swirl) as it is', () => {
  const stirred = { ...base, routes: { ...base.routes, 'hits|0|stir': 0.7, 'loudness|-|flow': 0.5, 'swell|-|swirl': -0.25 } } as VisualSettings;
  for (const scope of ['all', 'movement'] as const) {
    const s = settingsOf(recipe, stirred, scope);
    assert.equal(s.routes['hits|0|stir'], 0.7, scope);
    assert.equal(s.routes['loudness|-|flow'], 0.5, scope);
    assert.equal(s.routes['swell|-|swirl'], -0.25, scope);
  }
});

test('a band time pinned stays through either roll, the others the recipe’s', () => {
  const pinnedBase = { ...base, pins: ['lighting:2', 'evolution:5', 'continuity:0'] } as unknown as VisualSettings;
  for (const scope of ['all', 'movement'] as const) {
    const recipe = recipeFor(7, ASPECTS);
    const got = settingsOf(recipe, pinnedBase, scope);
    assert.equal(got.lighting[2], pinnedBase.lighting[2], `${scope}: Lighting 2 kept`);
    assert.equal(got.evolution[5], pinnedBase.evolution[5], `${scope}: Evolution 5 kept`);
    assert.equal(got.continuity[0], pinnedBase.continuity[0], `${scope}: Continuity 0 kept`);
    assert.notEqual(got.lighting[1], pinnedBase.lighting[1], `${scope}: Lighting 1 rolled`);
  }
});

test('each column’s scale: rolled from three quarters to all, the movement’s four only in a double tap, a pinned one kept', () => {
  const all = settingsOf(recipe, base);
  for (const k of SCALE_KEYS) assert.ok(all[k] >= 0.75 && all[k] <= 1, `${k} ${all[k]}`);
  const mv = settingsOf(recipe, base, 'movement');
  for (const k of SCALE_KEYS) {
    if (['shape', 'lights', 'rotation', 'size'].includes(k)) assert.ok(mv[k] >= 0.75, `${k} rolled`);
    else assert.equal(mv[k], 0.2, `${k} kept`);
  }
  const pinned = { ...base, pins: ['lights', 'stir'] } as VisualSettings;
  for (const scope of ['all', 'movement'] as const) {
    const got = settingsOf(recipe, pinned, scope);
    assert.equal(got.lights, 0.2, `${scope}: Lights kept`);
    assert.equal(got.stir, 0.2, `${scope}: Stir kept`);
  }
});

test('a recipe from before the columns’ scales gives all of each', () => {
  const old = { ...recipe, settings: { ...recipe.settings, scales: undefined } } as unknown as typeof recipe;
  const got = settingsOf(old, base);
  for (const k of SCALE_KEYS) assert.equal(got[k], 1, k);
});

test('a roll leaves the look’s own columns (65.1) as they are', () => {
  const routed = { ...base, routes: { ...base.routes, 'level|0|fx1K1': 0.5, 'beat|-|final': 1, 'swell|-|fx2Amount': 0.7 } } as VisualSettings;
  for (const scope of ['all', 'movement'] as const) {
    const s = settingsOf(recipe, routed, scope);
    assert.equal(s.routes['level|0|fx1K1'], 0.5, scope);
    assert.equal(s.routes['beat|-|final'], 1, scope);
    assert.equal(s.routes['swell|-|fx2Amount'], 0.7, scope);
  }
});
