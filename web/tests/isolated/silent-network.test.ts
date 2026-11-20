// A network that drops what is sent never fails a request, it only never answers (0.16.0 on the tablet, 2026-11-08: the app opened
// with Cratarr out of reach and Home loaded for good, as in the app no service worker gives up for it). A request that waits asks
// whether Cratarr answers at all: no answer, and it fails as out of reach, the page away, never an empty answer ("no results" on a
// silent network); an answer, and it waits on for a busy Cratarr (lib/api's `watch`, the real one).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { getHomeNew, UNREACHED, watch } = await import('../../src/lib/api.ts');
const { reach, reached } = await import('../../src/lib/reach.svelte.ts');
const { cratarrBusy, waitingFor } = await import('../../src/lib/waiting.svelte.ts');

watch.afterMs = 30;
watch.probeMs = 30;
// RC01: a health check that does not answer in time proves nothing alone; nothing heard for `silentMs` is out of reach.
watch.silentMs = 150;
watch.mostMs = 5_000;

/** A network where `health` says how /api/health fares and every other request answers after `slowMs`, or never (null). */
function network(health: 'silent' | 'ok', slowMs: number | null) {
  const asked: string[] = [];
  globalThis.fetch = ((url: string, init?: RequestInit) => {
    asked.push(url);
    const silent = () => new Promise<Response>((_, reject) => init?.signal?.addEventListener('abort', () => reject(init.signal!.reason)));
    if (url === '/api/health') return health === 'ok' ? Promise.resolve(new Response('{"status":"ok"}')) : silent();
    if (slowMs === null) return silent();
    return new Promise<Response>((r) => setTimeout(() => r(new Response(JSON.stringify({ albums: [{ id: 'al-1' }] }))), slowMs));
  }) as typeof fetch;
  return asked;
}

test('on a silent network a read fails as out of reach, the page away, and never as an empty answer', async () => {
  reached();
  const asked = network('silent', null);
  const started = Date.now();
  await assert.rejects(getHomeNew(), (e: Error) => e.message === UNREACHED, 'out of reach, not an answer of nothing');
  assert.equal(reach.away, true, 'the page goes on as away');
  assert.ok(Date.now() - started < 2_000, 'in moments, not for good');
  // Its health asked again until nothing was heard for its while (RC01), then let go.
  assert.ok(Date.now() - started >= watch.silentMs, 'not at the first health check that did not answer in time');
  assert.equal(asked[0], '/api/home/new');
  assert.ok(asked.length >= 3 && asked.slice(1).every((u) => u === '/api/health'), asked.join(', '));
});

test('a busy Cratarr that answers its health is waited for, however long its answer takes', async () => {
  reached();
  const asked = network('ok', 150);
  const home = await getHomeNew();
  assert.deepEqual(home, { albums: [{ id: 'al-1' }] });
  assert.equal(reach.away, false);
  assert.ok(asked.filter((u) => u === '/api/health').length >= 2, 'asked again while it waited');
});

test('an answer in time asks nothing more', async () => {
  reached();
  const asked = network('silent', 5);
  await getHomeNew();
  assert.deepEqual(asked, ['/api/home/new']);
});

test('while a busy Cratarr is waited for, every loader says so in its line, and no longer than it waits', async () => {
  reached();
  network('ok', 150);
  assert.equal(waitingFor(), null, 'an answer on its way says nothing more');
  const home = getHomeNew();
  await new Promise((r) => setTimeout(r, 80));
  assert.equal(cratarrBusy.asks, 1);
  assert.equal(waitingFor(), 'Waiting for Cratarr');
  await home;
  assert.equal(cratarrBusy.asks, 0);
  assert.equal(waitingFor(), null);
});

test('a silent network is no busy Cratarr: its wait ends as out of reach, nothing left counted', async () => {
  reached();
  network('silent', null);
  await assert.rejects(getHomeNew());
  assert.equal(cratarrBusy.asks, 0);
});
