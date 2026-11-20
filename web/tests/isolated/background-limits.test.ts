// RC01 (0.16.2's desktop app, 2026-11-18): after a sleep the status line said "Cratarr server unreachable" for hours while Cratarr
// was well, and no /api/status came from the app for 30 minutes; the same day Fetch said it could not reach production while it
// answered in 1.5 to 5.6 s. Its tests, red first on lib/api and lib/reach as they were at 7da786c91: a hard limit on background
// requests, slow is busy and not away, a look on wake past any hold, and the hold said in the console. (The poll that can't stop is
// tests/poll-every.test.ts.) The real modules, on tests/harness/pageGlobals.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const { getStatus, notAnswered, tookTooLong, UNREACHED, watch } = await import('../../src/lib/api.ts');
const { reach, reached, refused, refusedFor, wake } = await import('../../src/lib/reach.svelte.ts');
const { cratarrBusy } = await import('../../src/lib/waiting.svelte.ts');

watch.afterMs = 20;
watch.probeMs = 20;
watch.silentMs = 200;
watch.mostMs = 300;

const STATUS = { connected: true, problems: [], sources: [], version: '0.18.0' };

/** A network where /api/health is answered as `health` says (`slow`: after 60 ms, past its wait) and /api/status after
 *  `statusMs`, or never (null); every request's signal heeded, as a browser's fetch does. Returns what was asked. */
function network(health: 'ok' | 'slow' | 'silent' | 403, statusMs: number | null) {
  const asked: string[] = [];
  globalThis.fetch = ((url: string, init?: RequestInit) => {
    asked.push(url);
    return new Promise<Response>((resolve, reject) => {
      init?.signal?.addEventListener('abort', () => reject(init.signal!.reason));
      if (url === '/api/health') {
        if (health === 'ok') resolve(new Response('{"status":"ok"}'));
        else if (health === 403) resolve(new Response('', { status: 403 }));
        else if (health === 'slow') setTimeout(() => resolve(new Response('{"status":"ok"}')), 60);
        return;
      }
      if (statusMs !== null) setTimeout(() => resolve(new Response(JSON.stringify(STATUS))), statusMs);
    });
  }) as typeof fetch;
  return asked;
}

test('a background request has a hard limit even while Cratarr answers its health: let go as busy, not away', { timeout: 5_000 }, async () => {
  reached();
  const asked = network('ok', null);
  const started = Date.now();
  await assert.rejects(getStatus(), (e: unknown) => tookTooLong(e) && !notAnswered(e), 'busy, never "cannot reach"');
  const took = Date.now() - started;
  assert.ok(took >= watch.mostMs - 5 && took < 2_000, `at its limit, not for ever: ${took} ms`);
  assert.equal(reach.away, false, 'Cratarr answered: the page is not away');
  assert.equal(cratarrBusy.asks, 0, 'nothing left counted as waiting');
  assert.ok(asked.filter((u) => u === '/api/health').length >= 2, 'its health asked while it waited');
});

test('a health check slower than its wait proves nothing: a slow Cratarr is waited for, not called away', { timeout: 5_000 }, async () => {
  reached();
  network('slow', 150);
  assert.deepEqual(await getStatus(), STATUS);
  assert.equal(reach.away, false);
});

test('a silent network still ends as out of reach, once nothing was heard for its while', { timeout: 5_000 }, async () => {
  reached();
  network('silent', null);
  const started = Date.now();
  await assert.rejects(getStatus(), (e: Error) => e.message === UNREACHED);
  assert.ok(Date.now() - started >= watch.silentMs, 'never at the first look that did not answer in time');
  assert.equal(reach.away, true);
  assert.equal(cratarrBusy.asks, 0);
});

test("the proxy's bare refusal of the health check is out of reach at once", { timeout: 5_000 }, async () => {
  reached();
  network(403, null);
  const started = Date.now();
  await assert.rejects(getStatus(), (e: Error) => e.message === UNREACHED);
  assert.ok(Date.now() - started < watch.silentMs);
});

test('the page says in the console why background requests are held, once a hold, and a renewed hold again', { timeout: 5_000 }, async () => {
  const said: string[] = [];
  const warn = console.warn;
  console.warn = (...a: unknown[]) => void said.push(a.map(String).join(' '));
  try {
    network('ok', 0);
    refused();
    await assert.rejects(getStatus(), (e: Error) => e.message === UNREACHED, 'held: nothing asked');
    await assert.rejects(getStatus());
    assert.equal(said.length, 1, 'once a hold');
    assert.match(said[0], /background requests held back: refused \(401 or 403\), for \d+ s more/);
    await new Promise((r) => setTimeout(r, 5));
    refused();
    await assert.rejects(getStatus());
    assert.equal(said.length, 2, 'a renewed hold is said again: the log shows each 401 or 403 that renewed it');
  } finally {
    console.warn = warn;
  }
});

test('on waking the page looks for Cratarr once, past any hold; its answer lifts the hold, the bare refusal keeps it', { timeout: 5_000 }, async () => {
  refused();
  assert.ok(refusedFor() > 0);
  let asked = network('ok', 0);
  const t0 = Date.now();
  assert.equal(await wake(t0), true);
  assert.deepEqual(asked, ['/api/health'], 'one look, past the hold');
  assert.equal(refusedFor(), 0, 'Cratarr answered: no ban stands');
  assert.equal(reach.away, false);
  assert.deepEqual(await getStatus(), STATUS, 'the background requests go again');
  // The proxy's bare refusal: the hold stands.
  refused();
  asked = network(403, 0);
  assert.equal(await wake(t0 + 61_000), false);
  assert.ok(refusedFor() > 0);
  // Once a minute at most: a page shown again and again knocks once.
  asked = network('ok', 0);
  assert.equal(await wake(t0 + 62_000), false);
  assert.deepEqual(asked, []);
});

test('the network back and the page shown again are what wake it', () => {
  const src = readFileSync(new URL('../../src/lib/reach.svelte.ts', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
  assert.match(src, /addEventListener\('online', \(\) => \{\n    networkSaid\(true\);\n    void wake\(\);\n  \}\);/);
  assert.match(src, /document\.addEventListener\('visibilitychange', \(\) => \{\n    if \(!document\.hidden\) void wake\(\);\n  \}\);/);
});
