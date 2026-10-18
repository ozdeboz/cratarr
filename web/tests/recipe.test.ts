// What the dice make of a recipe (recipe.ts settingsOf): everything, or the movement only (the double tap), and what
// the owner pinned kept either way: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MOVEMENT, recipeFor, settingsOf } from '../src/lib/visuals/recipe.ts';
import { ASPECTS } from '../src/lib/visuals/routes.ts';
import type { VisualSettings } from '../src/lib/visuals/settings.svelte.ts';

const six = (v: number) => Array<number>(6).fill(v);
const base = {
  points: 19,
  steps: 3,
  resolution: null,
  fps: null,
  brightness: 1,
  wild: 0.5,
  colours: 0,
  pins: [] as string[],
  routes: { 'track|-|look': -1, 'swell|-|fluid': 0.5, 'loudness|-|hue': 0.3, 'loudness|-|size': 0.9, 'level|0|shape': 0.4 },
  flash: 0.33,
  speed: 0.44,
  intensity: 0.55,
  fluid: 0.66,
  smoothing: 0.11,
  floor: 0.12,
  depth: 0.13,
  lighting: six(0.5),
  evolution: six(0.5),
  continuity: six(0.5),
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
  assert.deepEqual([mv.intensity, mv.fluid, mv.smoothing, mv.floor, mv.depth], [0.55, 0.66, 0.11, 0.12, 0.13]);
  assert.notEqual(mv.flash, 0.33);
  assert.notEqual(mv.speed, 0.44);
  assert.notEqual(mv.lighting[0], 0.5);
  assert.notEqual(mv.evolution[0], 0.5);
  assert.notEqual(mv.continuity[0], 0.5);
});

test('what is pinned stays, in either roll', () => {
  const pinned = { ...base, pins: ['flash', 'route:loudness|-|size', 'column:shape'] } as VisualSettings;
  for (const scope of ['all', 'movement'] as const) {
    const s = settingsOf(recipe, pinned, scope);
    assert.equal(s.flash, 0.33, scope);
    assert.equal(s.routes['loudness|-|size'], 0.9, scope);
    assert.equal(s.routes['level|0|shape'], 0.4, scope);
    for (const k of Object.keys(s.routes).filter((k) => columnOf(k) === 'shape')) assert.equal(s.routes[k], pinned.routes[k], `${scope} ${k}`);
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
