// Module help (module-help.ts, decision 0057): every declared module type has its line, and every port and knob has its
// own line, help.ts's text, or is plain by its label, so a new module cannot ship without help: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { EXTRA_TYPES } = await import('../src/lib/visuals/registry.ts');
const { ENGINE_MODULES, GRAPH_MODULES, PICTURE_MODULES, polyKnobs, portsOf, showPartTypes } = await import('../src/lib/visuals/ports.ts');
const { HELP } = await import('../src/lib/visuals/help.ts');
const { HELP_KEY, PLAIN, helpFor, isPlain, moduleHelp, portHelp } = await import('../src/lib/visuals/module-help.ts');

const chain = cratarrTypes(RANGES, DEFAULTS);
const all = [...EXTRA_TYPES, ...chain, ...ENGINE_MODULES, ...GRAPH_MODULES, ...PICTURE_MODULES, ...showPartTypes(chain.find((t) => t.type === 'show')!)];
const types = new Map(all.map((t) => [t.type, t]));
/** Every id a type has a port or a knob by (a polyphonic knob once). */
const idsOf = (type: string) => {
  const t = types.get(type)!;
  const p = portsOf(t);
  return new Set([...p.in, ...p.out].map((x) => x.id).concat(polyKnobs(t).knobs.map((k) => k.id)));
};

test('every declared module type has its line', () => {
  for (const t of all) assert.ok(moduleHelp[t.type], `${t.type}: no line`);
  assert.ok(helpFor('drift') && helpFor('flash') === moduleHelp.accent, 'a renamed type reads as the one it is now');
});

test('every port and knob has a line, help.ts’s text, or is plain by its label', () => {
  for (const t of all)
    for (const id of idsOf(t.type)) assert.ok(helpFor(t.type, id) || isPlain(t.type, id), `${t.type}.${id}: no help`);
  // A polyphonic knob's channel reads as its knob.
  assert.equal(helpFor('flame', 'lighting.3'), HELP.lighting);
  assert.ok(helpFor('shapes', 'count:gain') && helpFor('flame', 'form:next'), 'a knob’s gain and next inputs');
});

test('no line, pointer or plain mark for a port or type that is not there', () => {
  for (const [table, by] of [['portHelp', portHelp], ['HELP_KEY', HELP_KEY], ['PLAIN', PLAIN]] as const)
    for (const [type, entries] of Object.entries(by)) {
      assert.ok(types.has(type), `${table}: no type ${type}`);
      const ids = idsOf(type);
      for (const id of Array.isArray(entries) ? entries : Object.keys(entries)) assert.ok(ids.has(id), `${table}: ${type}.${id} is not a port`);
    }
  for (const type of Object.keys(moduleHelp)) assert.ok(type === 'device' || types.has(type), `moduleHelp: no type ${type}`);
});

test('a pointer into help.ts finds its text; a type’s own line comes first', () => {
  for (const [type, keys] of Object.entries(HELP_KEY)) for (const [id, key] of Object.entries(keys)) assert.ok(HELP[key], `${type}.${id} → ${key}: not in help.ts`);
  assert.equal(helpFor('flame', 'form'), HELP.form);
  // Where help.ts still tells the old architecture (routes, the governor, Lighting), the type's own line comes first.
  assert.equal(helpFor('blur', 'smoothing'), portHelp.blur.smoothing);
  assert.equal(helpFor('kaleidoscope-engine', 'amount'), portHelp['kaleidoscope-engine'].amount);
  assert.equal(helpFor('music', 'hits'), portHelp.music.hits);
  assert.equal(helpFor('music', 'level'), HELP['signal:level']);
  assert.equal(helpFor('shapes', 'spread'), portHelp.shapes.spread, 'a Shapes knob is not the flame’s by its id');
});

test('one line each, short', () => {
  const lines = [...Object.values(moduleHelp), ...Object.values(portHelp).flatMap((p) => Object.values(p))];
  for (const l of lines) {
    assert.ok(!l.includes('\n') && l.length <= 130, `too long: ${l}`);
    assert.match(l, /^[A-Z0-9].*\.$/, `a sentence: ${l}`);
  }
});
