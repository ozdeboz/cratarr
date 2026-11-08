// What a device goes by, as those who keep it tell it: lib/output's plays not told yet, lib/away's tracks that play here, and lib/crates
// reading again when reads move. The real modules, with Cratarr away.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as wait } from 'node:timers/promises';
import '../harness/pageGlobals.ts';
import '../harness/fakeIdb.ts';

const g = globalThis as Record<string, unknown>;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
g.MediaSource = { isTypeSupported: () => true };
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });

const asked: string[] = [];
globalThis.fetch = (async (input: string) => {
  asked.push(String(input));
  throw new TypeError('Failed to fetch');
}) as typeof fetch;

const play = (id: string, account: string) => ({ id, source: 'cratarr', track: 'tr-101', at: 1_800_000_000_000, ended: 'finished', counts: true, heard_secs: 100, length_secs: 100, player: null, context: null, account });
localStorage.setItem('cratarr-who', '"ann"');
localStorage.setItem('cratarr-kept-plays', JSON.stringify([play('hers', 'ann'), play('his', 'bob')]));

const { reach } = await import('../../src/lib/reach.svelte.ts');
reach.away = true;
const { offline } = await import('../../src/lib/offline.svelte.ts');
const kept = (source: string, id: string) => ({ source, id, name: id, artist: null, album: null, album_id: null }) as (typeof offline.kept)[string];
offline.kept = { 'cratarr/tr-100': kept('cratarr', 'tr-100'), 'bandcamp/tr-100': kept('bandcamp', 'tr-200') };

const { heldHere } = await import('../../src/lib/deviceAnswers.svelte.ts');
await import('../../src/lib/away.svelte.ts');
await import('../../src/lib/output.svelte.ts');
const { crates } = await import('../../src/lib/crates.svelte.ts');
const { whenReadsMove } = await import('../../src/lib/deviceIndex.svelte.ts');
await wait(20);

test('the plays a device counts are those waiting of the account signed in', () => {
  assert.deepEqual(
    heldHere.plays().map((p) => (p as { id: string }).id),
    ['hers'],
  );
});

test('what plays here is what is kept of Cratarr\'s own, by its id alone', () => {
  assert.deepEqual(heldHere.tracks(), ['tr-100']);
});

test('reads moving, a list of crates the page has is read again, and one it has not is left', async () => {
  const again = () => {
    asked.length = 0;
    for (const f of whenReadsMove) f();
    return wait(10);
  };
  await again();
  assert.deepEqual(asked.filter((a) => a.startsWith('/api/crates')), [], 'nobody looked yet');
  crates.list = [];
  await again();
  assert.deepEqual(asked.filter((a) => a.startsWith('/api/crates')), ['/api/crates']);
});
