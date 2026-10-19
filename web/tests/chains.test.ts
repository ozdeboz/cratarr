// The preset chains (chains.ts, decision 0056; the owner after 70: "a few sensible chains as presets, the ability to
// start from scratch"): few cables each, every one a route there is, every setting within its range, kept as they are.
import { test } from 'node:test';
import assert from 'node:assert/strict';

// settings.svelte.ts makes its state with Svelte's rune at load; outside Svelte it is the value itself.
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES, keptFrom } = await import('../src/lib/visuals/settings.svelte.ts');
const { CHAINS, SCRATCH, chainOf, chainRoutes } = await import('../src/lib/visuals/chains.ts');
const { ASPECTS, BAND_SIGNALS, TRACK_SIGNALS } = await import('../src/lib/visuals/routes.ts');
const { extraFrom } = await import('../src/lib/visuals/patch.ts');
const { extraType } = await import('../src/lib/visuals/registry.ts');

const ALL = [...CHAINS, SCRATCH];

test('Calm, Pulse and Flow, and from scratch: lowercase ids, each found by its id', () => {
  assert.deepEqual(
    CHAINS.map((c) => c.id),
    ['calm', 'pulse', 'flow'],
  );
  for (const c of ALL) {
    assert.match(c.id, /^[a-z0-9][a-z0-9-]*$/);
    assert.equal(chainOf(c.id), c);
  }
  assert.equal(chainOf('nope'), undefined);
});

test('few cables, each a route there is (its signal, its band, its column)', () => {
  for (const c of ALL) {
    const keys = Object.keys(c.routes);
    assert.ok(keys.length <= 8, `${c.id}: ${keys.length} cables`);
    for (const k of keys) {
      const [signal, band, aspect] = k.split('|');
      assert.ok((ASPECTS as readonly string[]).includes(aspect), `${c.id}: ${k}`);
      if (band === '-') assert.ok((TRACK_SIGNALS as readonly string[]).includes(signal), `${c.id}: ${k}`);
      else assert.ok((BAND_SIGNALS as readonly string[]).includes(signal) && Number(band) >= 0 && Number(band) < 6, `${c.id}: ${k}`);
    }
  }
  assert.deepEqual(SCRATCH.routes, {});
});

test("every setting within its range; the presets draw the flame, from scratch nothing; never the look", () => {
  const LOOK = ['form', 'detail', 'twist', 'warp', 'symmetry', 'mirror', 'final', 'family', 'hue', 'lightness', 'vividness', 'spread', 'purity'];
  for (const c of ALL)
    for (const [k, v] of Object.entries(c.settings ?? {})) {
      assert.ok(!LOOK.includes(k), `${c.id} sets the look's ${k}`);
      const range = (RANGES as Record<string, [number, number]>)[k] ?? (k === 'flameLayer' ? [0, 1] : null);
      assert.ok(range && v >= range[0] && v <= range[1], `${c.id}.${k} = ${v}`);
    }
  for (const c of CHAINS) assert.equal(c.settings?.flameLayer, 1);
  assert.equal(SCRATCH.settings?.flameLayer, 0);
});

test('each chain is kept as it is: its routes (the defaults it leaves out kept off) through keptFrom, its extras through extraFrom', () => {
  for (const c of ALL) {
    const kept = keptFrom(JSON.parse(JSON.stringify({ ...DEFAULTS, routes: chainRoutes(c), extra: c.extra })));
    const on = Object.fromEntries(Object.entries(kept.routes).filter(([, v]) => v !== 0));
    assert.deepEqual(on, c.routes, c.id);
    assert.deepEqual(extraFrom(JSON.parse(JSON.stringify(c.extra)), extraType), c.extra, c.id);
  }
});
