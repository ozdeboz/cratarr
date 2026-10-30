// The five categories (rack.ts, decision 0057 §3b): what each module does with signals; the effects in flow order:
// npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { EXTRA_TYPES } = await import('../src/lib/visuals/registry.ts');
const { ENGINE_MODULES, GRAPH_MODULES, showPartTypes } = await import('../src/lib/visuals/ports.ts');
const { byCategory, categoryOf } = await import('../src/lib/visuals/rack.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');

const chain = cratarrTypes(RANGES, DEFAULTS);
const types = Object.fromEntries([...EXTRA_TYPES, ...chain, ...ENGINE_MODULES, ...GRAPH_MODULES, ...showPartTypes(chain.find((t) => t.type === 'show')!)].map((t) => [t.type, t]));

test('every module type has one of the five categories', () => {
  const cat = (type: string) => categoryOf(types[type]);
  assert.equal(cat('flame'), 'sources');
  assert.equal(cat('shapes'), 'sources');
  assert.equal(cat('fold'), 'effects');
  assert.equal(cat('warp'), 'effects');
  assert.equal(cat('mix'), 'mixers');
  assert.equal(cat('feedback'), 'mixers');
  assert.equal(cat('lfo'), 'modulators');
  assert.equal(cat('adsr'), 'modulators');
  assert.equal(cat('music'), 'io');
  assert.equal(cat('screen'), 'io');
});

test("today's chain by category: the flame a source, Show an effect device, the modulators, Music in", () => {
  const by = byCategory(toV2(JSON.parse(JSON.stringify(DEFAULTS))), types);
  const ids = (c: keyof typeof by) => by[c].map((m) => m.id);
  assert.ok(ids('sources').includes('flame'));
  assert.ok(ids('effects').includes('show'), JSON.stringify(ids('effects')));
  for (const id of ['envelope', 'clock', 'accent']) assert.ok(ids('modulators').includes(id), id);
  assert.ok(ids('io').includes('music'));
});

test('the effects in flow order', () => {
  const m = (id: string, type: string) => ({ id, type, knobs: {} });
  const c = (id: string, from: string, to: string) => ({ id, from: { module: from, port: 'picture' }, to: { module: to, port: 'picture' }, value: 1, mode: 'swing' as const });
  const p = { v: 2 as const, modules: [m('k', 'keep'), m('f', 'fold'), m('w', 'warp')], cables: [c('1', 'w', 'f'), c('2', 'f', 'k')], macros: [], pins: [] };
  assert.deepEqual(byCategory(p, types).effects.map((x) => x.id), ['w', 'f', 'k']);
});
