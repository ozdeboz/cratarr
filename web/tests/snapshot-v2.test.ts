// A snapshot as the switch keeps it: the patch whole, back as it was; an older one converted: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const state = <T>(v: T) => v;
state.raw = <T>(v: T) => v;
state.snapshot = <T>(v: T) => structuredClone(v);
(globalThis as Record<string, unknown>).$state = state;
const { snapshotOfPatch, patchOfSnapshot } = await import('../src/lib/visuals/snapshot-v2.ts');

test('a patch kept and back; a settings snapshot converted', () => {
  const p = { v: 2 as const, modules: [{ id: 'noise-1', type: 'noise', knobs: { scale: 0.3 } }], cables: [], macros: [], pins: ['category:sources'] };
  assert.deepEqual(patchOfSnapshot(snapshotOfPatch(p)), p);
  const old = patchOfSnapshot({ v: 3, hue: 0.2 });
  assert.equal(old.v, 2);
  assert.equal(old.modules.find((m) => m.type === 'flame')!.knobs.hue, 0.2);
});

// 71.1: every patch snapshot failed to recall from the Snapshots sheet and the panel's Presets. Their lists are Svelte's
// $state, so a snapshot's settings come as its reactive proxy, and the store's structuredClone threw DataCloneError on
// them (the tap did nothing). Each form a snapshot has, as the page's list gives it (Svelte's own proxy), recalled into
// the store and saved back.
test('every form of snapshot recalls from a reactive list into the store, and saves back as it was', async () => {
  const { proxy } = await import('svelte/internal/client');
  const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
  const { toV2 } = await import('../src/lib/visuals/v2.ts');
  const { emptyRack } = await import('../src/lib/visuals/visuals-patch.svelte.ts');
  const { newStore, replace } = await import('../src/lib/visuals/patch-store.ts');
  const rack = toV2(structuredClone(DEFAULTS));
  assert.ok(rack.modules.some((m) => 'patch' in m), 'the default rack has devices');
  const small = { v: 2 as const, modules: [{ id: 'noise-1', type: 'noise', knobs: { scale: 0.3 } }, { id: 'screen', type: 'screen', knobs: {} }], cables: [], macros: [], pins: [] };
  const forms: Record<string, Record<string, unknown>> = {
    'v4, the rack with its devices': snapshotOfPatch(rack),
    'v4, a small rack': snapshotOfPatch(small),
    'v4, the empty rack': snapshotOfPatch(emptyRack()),
    v3: { v: 3, quality: 0.6, haze: 0.2, hue: 0.4, routes: { 'level|0|shape': 1 } },
    v2: { v: 2, quality: 0.5, hue: 0.3, routes: { 'beat|-|size': 0.1 } },
    v1: { v: 1, hue: 0.2, routes: { 'loudness|-|size': 1 } },
  };
  const list = proxy(Object.entries(forms).map(([name, settings], i) => ({ id: String(i), name, settings: JSON.parse(JSON.stringify(settings)) })));
  assert.throws(() => structuredClone(list[0].settings), /could not be cloned/, "the list's snapshots are proxies, as on the page");
  for (const snap of list) {
    const p = patchOfSnapshot(snap.settings);
    const plain = patchOfSnapshot(forms[snap.name]);
    assert.deepEqual(p, plain, `${snap.name}: as from a plain snapshot`);
    const store = replace(newStore(emptyRack()), p);
    assert.deepEqual(store.now, plain, `${snap.name}: in the store`);
    assert.deepEqual(patchOfSnapshot(JSON.parse(JSON.stringify(snapshotOfPatch(store.now)))), plain, `${snap.name}: saved and recalled`);
  }
});
