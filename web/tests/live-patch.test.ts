// The store's patch as the engine runs it (engine/live-patch.ts): the same object while only values change (a knob's
// drag), its values copied in; a new one when the wiring changes. The engine over it gives what it gives over the store's.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
const { LivePatch } = await import('../src/lib/visuals/engine/live-patch.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;

/** A change as the store makes it: a new patch, `f` applied to a deep copy. */
const changed = (p: PatchV2, f: (q: PatchV2) => void): PatchV2 => {
  const q = structuredClone(p);
  f(q);
  return q;
};

test('a knob dragged: the same patch for the engine, the knob as dragged; a cable added: a new one', () => {
  const live = new LivePatch();
  const a = toV2(structuredClone(DEFAULTS));
  const first = live.update(a);
  const b = changed(a, (q) => {
    q.modules.find((m) => m.id === 'tunnel')!.knobs.tunnel = 0.77;
    q.cables[0].value = 0.123;
  });
  const second = live.update(b);
  assert.equal(second, first, 'the same object');
  assert.equal(second.modules.find((m) => m.id === 'tunnel')!.knobs.tunnel, 0.77);
  assert.equal(second.cables.find((x) => x.id === a.cables[0].id)!.value, 0.123);
  // Inside the Show device, flattened.
  const c = changed(b, (q) => {
    const show = q.modules.find((m) => m.id === 'show') as unknown as { patch: PatchV2 };
    show.patch.modules.find((m) => m.type === 'curve')!.knobs.haze = 0.4;
  });
  assert.equal(live.update(c), first);
  assert.equal(first.modules.find((m) => m.id === 'show/curve')!.knobs.haze, 0.4);
  assert.equal(live.rebuilds, 1);
  const d = changed(c, (q) => q.cables.push({ id: 'new', from: { module: 'music', port: 'level', channel: 1 }, to: { module: 'tunnel', port: 'tunnel' }, value: 0.5 }));
  const third = live.update(d);
  assert.notEqual(third, first);
  assert.ok(third.cables.some((x) => x.id === 'new'));
  assert.equal(live.rebuilds, 2);
});

test("the fluid's push gains from the patch's cables: what the routes' matrix and the extras' cables gave", async () => {
  const { fluidGains } = await import('../src/lib/visuals/engine/fluid-gains.ts');
  const { compile, heightOn, routeKey } = await import('../src/lib/visuals/routes.ts');
  const { flatten } = await import('../src/lib/visuals/v2.ts');
  const s = structuredClone(DEFAULTS);
  s.routes = {
    ...s.routes,
    [routeKey('loudness', null, 'flow')]: 0.8,
    [routeKey('beat', null, 'flow')]: { h1: [0.2, 0.9], h2: [0.6, 0.3], h: 0.7 },
    [routeKey('swell', null, 'swirl')]: 0.5,
    [routeKey('hits', 2, 'stir')]: 0.9,
    [routeKey('level', 4, 'stir')]: 0.6,
    [routeKey('downbeat', null, 'stir')]: 1,
  };
  const matrix = compile(s.routes, 6);
  const gain = fluidGains(flatten(toV2(s)));
  const asks: [string, number | null, 'stir' | 'flow' | 'swirl'][] = [['loudness', null, 'flow'], ['beat', null, 'flow'], ['swell', null, 'swirl'], ['section', null, 'swirl'], ['downbeat', null, 'stir'], ['section', null, 'stir'], ['beat', null, 'stir'], ...[0, 1, 2, 3, 4, 5].flatMap((b): [string, number, 'stir'][] => [['hits', b, 'stir'], ['level', b, 'stir']])];
  for (const [signal, band, aspect] of asks)
    for (const x of [0, 0.3, 0.7, 1]) {
      const want = heightOn(matrix, signal as never, band, aspect, x);
      // The matrix keeps its amounts as 32-bit floats.
      assert.ok(Math.abs(gain(signal, band, aspect, x) - want) < 1e-6, `${signal} ${band} ${aspect} at ${x}: ${gain(signal, band, aspect, x)} against ${want}`);
    }
});
