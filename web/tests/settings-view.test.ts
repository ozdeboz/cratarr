// A patch seen as today's settings (engine/settings-view.ts): toV2's mapping undone, setting for setting; a column's
// scale reads 1 (it is in the cables now); the chain found inside the Cratarr device too.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { SETTING_KNOB } = await import('../src/lib/visuals/patch.ts');
const { COLUMN_SCALES } = await import('../src/lib/visuals/routes.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
const { settingsOfPatch } = await import('../src/lib/visuals/engine/settings-view.ts');

const FOLDED = new Set(Object.entries(COLUMN_SCALES).filter(([a]) => !['stir', 'flow', 'swirl'].includes(a)).map(([, k]) => k));

test('toV2 undone: every setting as it was (a folded scale: 1, Effects in the Amounts), the bands’ times too, at the top level and in a device', () => {
  const s = structuredClone(DEFAULTS);
  Object.assign(s, { form: 412, hue: 0.37, tunnel: 0.4, dive: 0.3, fluid: 0.6, stir: 1.3, flash: 1.7, speed: 1.4, intensity: 0.8, effects: 0.6, haze: 0.2, smoothing: 0.5, fx1: 2, fx1Amount: 0.7, shape: 1.4, size: 0.8 });
  s.lighting = [0.3, 0.25, 0.2, 0.15, 0.1, 0.05];
  s.continuity = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
  s.evolution = [1, 2, 3, 4, 5, 6];
  const p = flatten(toV2(s));
  const inDevice = { ...p, modules: p.modules.map((m) => (/^(flame|fluid|tunnel|dive|trails|frame|fx\d|loop-effect|show\/.+)$/.test(m.id) ? { ...m, id: `cratarr/${m.id}` } : m)) };
  for (const patch of [p, inDevice]) {
    const view = settingsOfPatch(patch, DEFAULTS) as unknown as Record<string, unknown>;
    const was = s as unknown as Record<string, unknown>;
    for (const k of Object.keys(SETTING_KNOB)) {
      if (!(k in was)) continue;
      // Effects is the rack's macro on the effects' Amounts (the Whole frame gone): folded into them.
      const want = FOLDED.has(k) || k === 'effects' ? 1 : /^fx(\d|Loop)Amount$/.test(k) && (was[k] as number) > 0 ? 0 + ((was[k] as number) - 0) * s.effects : was[k];
      assert.deepEqual(view[k], want, k);
    }
    for (const k of ['lighting', 'continuity', 'evolution']) assert.deepEqual(view[k], was[k], k);
  }
});
