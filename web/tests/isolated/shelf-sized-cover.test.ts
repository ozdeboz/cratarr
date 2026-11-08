// The Downloads shelf's covers (lib/shelfStore, the real one, in the app's page; finding 86): asked of Cratarr at the shelf's own size,
// never whole, as a whole one is tens of MB decoded on a tablet before it is made small here.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as wait } from 'node:timers/promises';
import { effects } from '../harness/pageGlobals.ts';
import '../harness/fakeIdb.ts';

const g = globalThis as Record<string, unknown>;
const bridge: { postMessage(m: string): void; onmessage: ((e: { data: string }) => void) | null } = { postMessage: () => {}, onmessage: null };
g.CratarrApp = bridge;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
const worker = { postMessage: () => {} };
Object.defineProperty(globalThis, 'navigator', {
  value: { onLine: true, serviceWorker: { ready: Promise.resolve({ active: worker }), controller: worker, addEventListener() {} } },
  configurable: true,
});

/** Every cover the page asked Cratarr for. */
const covers: string[] = [];
const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json' } });
globalThis.fetch = (async (input: string) => {
  const path = String(input);
  if (path === '/api/me') return json({ name: 'ann', admin: false, permissions: {} });
  if (/^\/api\/albums\/[^/]+\/[^/]+\/cover/.test(path)) {
    covers.push(path);
    return new Response(new Uint8Array([0xff, 0xd8, 0xff, 0xd9]), { headers: { 'content-type': 'image/jpeg' } });
  }
  if (path.startsWith('/api/albums/')) return json({ title: 'A record', artist: null, year: null, genres: [], rows: [] });
  return json({});
}) as typeof fetch;

const { accountLearnt } = await import('../../src/lib/account.ts');
const { COVER_PX } = await import('../../src/lib/shelf.ts');
await import('../../src/lib/shelfStore.svelte.ts');

const fromApp = (m: Record<string, unknown>) => bridge.onmessage!({ data: JSON.stringify(m) });
const track = (album: string) => ({ source: 'cratarr', id: `t-${album}`, name: 'One', artist: null, album: 'A', album_id: album, added: 1, download: { id: 'x', state: 'done', bytes: 1, percent: 100 } });

test("the shelf asks Cratarr for a record's cover at its own size, and never for the whole one", async () => {
  accountLearnt('ann');
  await wait(80);
  fromApp({ type: 'downloads', tracks: [track('al-1')], bytes: 0, who: '@ann' });
  for (let round = 0; round < 6 && !covers.length; round++) {
    for (const run of effects) run();
    await wait(80);
  }
  assert.deepEqual(covers, [`/api/albums/cratarr/al-1/cover?size=${COVER_PX}`]);
});
