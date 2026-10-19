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
  assert.deepEqual(ENGINE_MODULES.map((m) => m.type), ['music', 'track', 'envelope', 'clock', 'flash', 'walk', 'safety']);
  const music = portsOf(ENGINE_MODULES[0]);
  assert.equal(music.out.find((x) => x.id === 'beat')?.type, 'trigger');
  assert.equal(music.out.find((x) => x.id === 'hits')?.channels, 6);
  assert.equal(music.out.find((x) => x.id === 'loudness')?.type, 'value');
  assert.deepEqual(portsOf(ENGINE_MODULES[2]).in[0].normal, { module: 'music', port: 'loudness' });
});

test('a cable fits by type: the same, or a trigger into a value', () => {
  assert.ok(fits('picture', 'picture') && fits('value', 'value') && fits('trigger', 'trigger') && fits('trigger', 'value'));
  assert.ok(!fits('value', 'picture') && !fits('picture', 'value') && !fits('value', 'trigger'));
});
