// Typed ports (ports.ts, decision 0057 step 1): every module type has them; today's hidden behaviours are declared as
// modules; cables fit by type: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { EXTRA_TYPES } = await import('../src/lib/visuals/registry.ts');
const { ENGINE_MODULES, fits, portsOf } = await import('../src/lib/visuals/ports.ts');

test('every module type has typed ports: a knob is a value input, a picture module takes and gives a picture', () => {
  for (const t of [...EXTRA_TYPES, ...cratarrTypes(RANGES, DEFAULTS), ...ENGINE_MODULES]) {
    const p = portsOf(t);
    const ids = p.in.map((x) => x.id);
    assert.equal(new Set(ids).size, ids.length, `${t.type}: an input twice`);
    for (const k of t.knobs.filter((k) => !k.device)) assert.equal(p.in.find((x) => x.id === k.id)?.type, 'value', `${t.type}.${k.id}`);
    if (t.place === 'module' || t.place === 'loop') assert.ok(p.in.some((x) => x.type === 'picture') && p.out.some((x) => x.type === 'picture'), t.type);
    if (t.place === 'source') assert.ok(!p.in.some((x) => x.type === 'picture') && p.out.some((x) => x.type === 'picture'), t.type);
    if (t.place === 'modulator') assert.ok(p.out.length > 0 && p.out.every((x) => x.type !== 'picture'), t.type);
  }
});

test("today's hidden behaviours are modules; the beats and hits are triggers, a band's signals six channels", () => {
  assert.deepEqual(ENGINE_MODULES.map((m) => m.type), ['music', 'track', 'envelope', 'clock', 'flash', 'walk', 'governor', 'taste']);
  const music = portsOf(ENGINE_MODULES[0]);
  assert.equal(music.out.find((x) => x.id === 'beat')?.type, 'trigger');
  assert.equal(music.out.find((x) => x.id === 'hits')?.channels, 6);
  assert.equal(music.out.find((x) => x.id === 'loudness')?.type, 'value');
  assert.deepEqual(portsOf(ENGINE_MODULES[2]).in[0].normal, { module: 'music', port: 'loudness' });
});

test('a cable fits by type: the same, or a trigger into a value', () => {
  assert.ok(fits('picture', 'picture') && fits('light', 'light') && fits('value', 'value') && fits('trigger', 'trigger') && fits('trigger', 'value'));
  assert.ok(!fits('light', 'picture') && !fits('value', 'picture') && !fits('picture', 'value') && !fits('value', 'trigger'));
});

test('v2: a picture cycle must pass through a Feedback', async () => {
  const { cyclesClosed, FEEDBACK, GRAPH_MODULES, MIX } = await import('../src/lib/visuals/ports.ts');
  const { WARP } = await import('../src/lib/visuals/loopmods.ts');
  const types = new Map([...GRAPH_MODULES, WARP].map((t) => [t.type, t]));
  const typeOf = (t: string) => types.get(t);
  const m = (id: string, type: string) => ({ id, type, knobs: {} });
  const c = (id: string, from: string, to: string, port = 'picture') => ({ id, from: { module: from, port: 'picture' }, to: { module: to, port }, value: 1, mode: 'swing' as const });
  const looped = { v: 2 as const, modules: [m('mix', 'mix'), m('warp', 'warp'), m('fb', 'feedback')], cables: [c('a', 'mix', 'fb', 'picture'), c('b', 'fb', 'warp'), c('c', 'warp', 'mix', 'in2')], macros: [], pins: [] };
  assert.ok(cyclesClosed(looped, typeOf));
  const open = { ...looped, cables: [c('a', 'mix', 'warp'), c('c', 'warp', 'mix', 'in2')] };
  assert.ok(!cyclesClosed(open, typeOf));
  assert.equal(FEEDBACK.type, 'feedback');
  assert.equal(MIX.ports.in.filter((p) => p.type === 'picture').length, 4);
});
