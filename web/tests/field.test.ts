// Field (field.ts): a flow with no sources or sinks that changes smoothly over time; npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
const { flowAt, FIELD_GLSL, WAVES } = await import('../src/lib/visuals/field.ts');

test('the Field’s flow neither bunches up nor thins out (no divergence), about 1 at its strongest, and moves on in time', () => {
  const h = 1e-4;
  let most = 0;
  for (let i = 0; i < 200; i++) {
    const [x, y, phase] = [Math.sin(i * 1.7) * 1.5, Math.cos(i * 2.3), i * 0.37];
    const div = (flowAt(x + h, y, 1.2, phase)[0] - flowAt(x - h, y, 1.2, phase)[0] + flowAt(x, y + h, 1.2, phase)[1] - flowAt(x, y - h, 1.2, phase)[1]) / (2 * h);
    assert.ok(Math.abs(div) < 1e-6, `divergence ${div} at ${x}, ${y}`);
    most = Math.max(most, Math.hypot(...flowAt(x, y, 1.2, phase)));
  }
  assert.ok(most > 0.3 && most <= 2, `strongest ${most}`);
  const a = flowAt(0.3, -0.2, 1.2, 0);
  const b = flowAt(0.3, -0.2, 1.2, 0.5);
  assert.ok(Math.hypot(a[0] - b[0], a[1] - b[1]) > 1e-3, 'it changes as its time runs on');
  assert.equal((FIELD_GLSL.match(/float k = /g) ?? []).length, WAVES.length, 'the GLSL sums the same waves');
});

test('its shape (Range, Roughness, Variety) keeps it free of divergence; Range spreads the swirls’ sizes', async () => {
  const { strengthsOf } = await import('../src/lib/visuals/field.ts');
  const shape = { range: 6, roughness: 1.4, variety: 0.8 };
  // A finer step than above: the smallest swirls are six times smaller here.
  const h = 1e-5;
  for (let i = 0; i < 200; i++) {
    const [x, y, phase] = [Math.sin(i * 1.3) * 1.5, Math.cos(i * 2.9), i * 0.41];
    const f = (a: number, b: number) => flowAt(a, b, 1.2, phase, undefined, shape);
    const div = (f(x + h, y)[0] - f(x - h, y)[0] + f(x, y + h)[1] - f(x, y - h)[1]) / (2 * h);
    assert.ok(Math.abs(div) < 1e-5, `divergence ${div} at ${x}, ${y}`);
  }
  const s = strengthsOf(undefined, { range: 4, roughness: 0, variety: 0 });
  assert.ok(Math.abs(s[0] - 1) < 1e-9 && Math.abs(s[5] - 0.25) < 1e-9, 'Roughness 0: the smallest as much weaker as it is smaller');
  assert.deepEqual(strengthsOf([1, 1, 1, 1, 1, 1], { range: 4, roughness: 1, variety: 0 }), [1, 1, 1, 1, 1, 1], 'Roughness 1: all alike');
});
