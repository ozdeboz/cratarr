// One module per Music output (engine/music-signals.ts): a `music-<signal>` module's `out` is Music's port of its
// signal, read by its own reading.
import { test } from 'node:test';
import assert from 'node:assert/strict';
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;

const { musicView } = await import('../src/lib/visuals/engine/music-signals.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;

test('a Music signal module reads as Music’s port of its signal, by its own reading', () => {
  const p: PatchV2 = {
    v: 2,
    modules: [
      { id: 'lvl', type: 'music-level', knobs: { reference: 2 } },
      { id: 'lfo', type: 'lfo', knobs: {} },
    ],
    cables: [{ id: 'c', from: { module: 'lvl', port: 'out', channel: 3 }, to: { module: 'lfo', port: 'rate' }, value: 1 }],
    macros: [],
    pins: [],
  };
  const v = musicView(p);
  assert.equal(musicView(p), v, 'once a patch object');
  assert.deepEqual(v.cables[0].from, { module: 'music', port: 'level', channel: 3 });
  assert.equal(v.modules.find((m) => m.type === 'music')!.knobs['reference.level'], 2);
  assert.ok(!v.modules.some((m) => m.id === 'lvl'));
  const none: PatchV2 = { v: 2, modules: [], cables: [], macros: [], pins: [] };
  assert.equal(musicView(none), none);
});
