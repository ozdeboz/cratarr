// The effect engines as modules (engine-module.ts, decision 0056): the effect at the Effect knob, amounts and knobs
// held to 0..1, a held effect easing back to rest as Amount nears 0, and every engine's every effect a layer shader.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { engineFrame, engineOf } from '../src/lib/visuals/engine-module.ts';
import { ENGINE_EFFECTS, ENGINES, effectOf, pictureShader } from '../src/lib/visuals/effects.ts';
import { ENGINE_TYPES } from '../src/lib/visuals/registry.ts';

test("every engine module type names an engine, and its Effect knob spans the engine's effects", () => {
  for (const t of ENGINE_TYPES) {
    const engine = engineOf(t.type);
    assert.ok(engine, t.type);
    const effect = t.knobs.find((k) => k.id === 'effect')!;
    assert.equal(effect.max, ENGINE_EFFECTS[engine!].length);
  }
  assert.equal(engineOf('no-engine'), null);
});

test('the Effect knob picks by place, rounded and held within the list; the rest held to 0..1', () => {
  const engine = ENGINES[0].key;
  const list = ENGINE_EFFECTS[engine];
  assert.equal(engineFrame(engine, { effect: 1.4, amount: 1 }, [0, 0]).id, list[0].id);
  assert.equal(engineFrame(engine, { effect: 2.6, amount: 1 }, [0, 0]).id, list[Math.min(2, list.length - 1)].id);
  assert.equal(engineFrame(engine, { effect: 99, amount: 1 }, [0, 0]).id, list[list.length - 1].id);
  const f = engineFrame(engine, { effect: 1, amount: 7, k1: -3, k2: Number.NaN }, [0, 0]);
  assert.deepEqual([f.amount, f.k[0], f.k[1]], [1, 0, 0.5]);
});

test("a held effect's time eases to rest as Amount nears 0; another's is as it runs", () => {
  for (const { key } of ENGINES)
    ENGINE_EFFECTS[key].forEach((e, i) => {
      const at = (amount: number) => engineFrame(key, { effect: i + 1, amount }, [2, 3]).time;
      if (e.holds) {
        assert.deepEqual(at(0), [0, 0], e.name);
        assert.deepEqual(at(0.05), [1, 1.5], e.name);
        assert.deepEqual(at(0.5), [2, 3], e.name);
      } else assert.deepEqual(at(0.01), [2, 3], e.name);
    });
});

test("every engine's every effect makes a layer shader that keeps its alpha", () => {
  for (const { key } of ENGINES)
    for (const e of ENGINE_EFFECTS[key]) {
      const s = pictureShader([e.id], true);
      assert.ok(s && s.includes('src.a') && s.includes(`fx_${effectOf(e.id)!.name}`), e.name);
      assert.ok(pictureShader([e.id])!.includes('1.0);'), 'the finish stays opaque');
    }
});
