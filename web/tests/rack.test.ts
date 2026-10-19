// The rack's groups (rack.ts, decision 0057 §4): by ports, the picture lane in flow order: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { EXTRA_TYPES } = await import('../src/lib/visuals/registry.ts');
const { ENGINE_MODULES, GRAPH_MODULES, showPartTypes } = await import('../src/lib/visuals/ports.ts');
const { lanesOf, portsFrom } = await import('../src/lib/visuals/rack.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');

const chain = cratarrTypes(RANGES, DEFAULTS);
const types = Object.fromEntries([...EXTRA_TYPES, ...chain, ...ENGINE_MODULES, ...GRAPH_MODULES, ...showPartTypes(chain.find((t) => t.type === 'show')!)].map((t) => [t.type, t]));

test('today\u2019s chain by lane: the flame a source, Show a picture device, the engine modules control', () => {
  const lanes = lanesOf(toV2(JSON.parse(JSON.stringify(DEFAULTS))), portsFrom(types));
  const ids = (l: keyof typeof lanes) => lanes[l].map((m) => m.id);
  assert.ok(ids('sources').includes('flame'), JSON.stringify(ids('sources')));
  assert.ok(ids('picture').includes('show'), JSON.stringify(ids('picture')));
  for (const id of ['music', 'envelope', 'clock', 'flash']) assert.ok(ids('control').includes(id), id);
});

test('the picture lane in flow order, a Feedback not counted as before what it feeds', () => {
  const m = (id: string, type: string) => ({ id, type, knobs: {} });
  const c = (id: string, from: string, to: string, port = 'picture') => ({ id, from: { module: from, port: 'picture' }, to: { module: to, port }, value: 1, mode: 'swing' as const });
  const p = {
    v: 2 as const,
    modules: [m('k', 'keep'), m('w', 'warp'), m('fb', 'feedback'), m('mix', 'mix')],
    cables: [c('1', 'mix', 'fb'), c('2', 'fb', 'w'), c('3', 'w', 'k'), c('4', 'k', 'mix', 'in2')],
    macros: [],
    pins: [],
  };
  const order = lanesOf(p, portsFrom(types)).picture.map((x) => x.id);
  // The loop read from its Feedback: last frame's picture, warped, kept, mixed with the new.
  assert.deepEqual(order, ['fb', 'w', 'k', 'mix']);
});
