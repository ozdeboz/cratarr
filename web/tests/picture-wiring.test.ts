// The picture side of a converted patch (picture-wiring.ts, through v2.ts's hook) and the passes it is drawn with
// (picture-graph.ts `stagesOf`), decision 0057 step 3: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
const { compile, stagesOf, STAGES, v2Types } = await import('../src/lib/visuals/picture-graph.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { EXAMPLES } = await import('../src/lib/visuals/chains.ts');
type VisualSettings = typeof DEFAULTS;

const types = v2Types(cratarrTypes(RANGES, DEFAULTS));
const extraOf = (id: string) => EXAMPLES.find((c) => c.id === id)!.extra;
const settings = (extra?: VisualSettings['extra']): VisualSettings => ({ ...JSON.parse(JSON.stringify(DEFAULTS)), ...(extra ? { extra } : {}) });
const layered = () => ({ ...extraOf('example-kaleido-layer'), layers: [...extraOf('example-kaleido-layer').layers, ...extraOf('example-window').layers], loop: extraOf('example-feedback').loop });

test("today's chain converted is whole: the flame's light loop, Show, the layers, the effects, the screen", () => {
  const c = compile(toV2(settings()), types);
  assert.deepEqual(c.errors, []);
  assert.deepEqual(
    c.order.map((n) => n.id),
    ['trails', 'fluid', 'tunnel', 'loop-effect', 'dive', 'flame', 'show/bend', 'show/blur', 'show/tonemap', 'show/curve', 'show/saturate', 'light-feedback', 'layers', 'fx1', 'fx2', 'fx3', 'screen'],
  );
  assert.deepEqual(stagesOf(c), [...STAGES]);
  // Bend and Blur on the light, Tone map its only converter.
  const bend = c.order.find((n) => n.id === 'show/bend')!;
  assert.deepEqual(bend.inputs.map((f) => [f.from.module, f.type]), [['flame', 'light']]);
});

test('layers and the picture’s own loop: mixed in their order, the loop’s Feedback after its readers', () => {
  const v = toV2(settings(layered()));
  const c = compile(v, types);
  assert.deepEqual(c.errors, []);
  const ids = c.order.map((n) => n.id);
  const at = (id: string) => ids.indexOf(id);
  assert.ok(at('shapes-kaleido') < at('layers') && at('window-source') < at('layers'));
  assert.ok(at('example-keep') < at('loop-mix') && at('loop-mix') < at('loop-feedback') && at('loop-feedback') < at('screen'));
  assert.deepEqual(c.order.find((n) => n.id === 'layers')!.inputs.map((f) => f.from.module), ['show/saturate', 'shapes-kaleido', 'window-source']);
  const mix = v.modules.find((m) => m.id === 'layers')!;
  assert.deepEqual([mix.knobs.mode1, mix.knobs.mode2, mix.knobs.mode3], [0, 1, 4], 'add, screen, window');
  assert.deepEqual(stagesOf(c), [...STAGES]);
});

test('passes out of the order this version draws them in are refused', () => {
  const c = compile(toV2(settings()), types);
  assert.equal(stagesOf({ ...c, order: [...c.order].reverse() }), null);
});
