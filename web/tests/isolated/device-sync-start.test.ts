// How sync goes in a page as it loads (lib/deviceSync, the real one, in a stand-in for a browser): what starts a look, and what does not.
import { afterEach, beforeEach, mock, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';
import '../harness/fakeIdb.ts';

const g = globalThis as Record<string, unknown>;
g.window = globalThis;
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
const connection = Object.assign(new EventTarget(), { type: 'wifi', saveData: false });
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, connection, storage: { persist: async () => true }, serviceWorker: { addEventListener() {} } }, configurable: true });
localStorage.setItem('cratarr-who', '"ann"');

const vector = (name: string) => new Uint8Array(readFileSync(new URL(`../../../server/rules/tests/vectors/index/${name}`, import.meta.url)));
const wasm = readFileSync(new URL('../../src/lib/wasm/index.wasm', import.meta.url));

/** A bundle of no covers yet, at rev 5, as a fresh server gives. */
const COVERS = new Uint8Array([...new TextEncoder().encode('CRXV'), ...new Uint8Array(new Uint32Array([1]).buffer), ...new Uint8Array(new BigInt64Array([5n]).buffer)]);
const WHOLES = ['/api/index', '/api/index/sounds', '/api/index/covers', '/api/index/own'];
/** What an untold look asks of a device holding all three: only the feed that builds nothing. */
const UNTOLD = ['/api/index/covers/changes?since=5'];

/** What Cratarr was asked of the library, and what it says at the log to a device that says where it stands. Whether the module came. */
let asked: string[] = [];
let moduleFetched = false;
let log: Record<string, unknown> = {};
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
globalThis.fetch = (async (input: string) => {
  const url = String(input);
  if (url.includes('index.wasm')) {
    moduleFetched = true;
    return new Response(wasm);
  }
  if (url.startsWith('/api/index')) asked.push(url);
  if (url === '/api/index') return new Response(vector('whole.bin'));
  if (url === '/api/index/sounds') return new Response(vector('sounds.bin'));
  if (url === '/api/index/covers') return new Response(COVERS);
  if (url === '/api/index/own') return new Response(vector('own.bin'));
  if (url.startsWith('/api/index')) return new Response(null, { status: 204 });
  if (url.startsWith('/api/changes')) return new Response(JSON.stringify({ rev: 1, changes: [], more: false, ...(url.includes('index=5') ? log : {}) }));
  if (url.startsWith('/api/capabilities')) return new Response(JSON.stringify({ capabilities: ['changes'], version: '1' }));
  return new Response('{}', { headers: { 'content-type': 'application/json' } });
}) as typeof fetch;

mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 2_000_000_000 });
const { accountForgotten, accountLearnt, whoOf } = await import('../../src/lib/account.ts');
const { writePart } = await import('../../src/lib/indexStore.ts');
const { ANEW_AFTER_MS, FIRST_LOOK_MS, indexSync, lookDone, stopLooking } = await import('../../src/lib/deviceSync.svelte.ts');
const { networkSaid, reached } = await import('../../src/lib/reach.svelte.ts');
const { readChangesNow } = await import('../../src/lib/changes.svelte.ts');
const { loadCapabilities } = await import('../../src/lib/capabilities.svelte.ts');
const page = document as unknown as EventTarget & { hidden: boolean };

/** Turns enough for what began to end: the fake store answers on the next one. */
const settle = async () => {
  await lookDone();
  for (let i = 0; i < 20; i++) await new Promise((go) => setImmediate(go));
  await lookDone();
};
async function gone(ms: number) {
  mock.timers.tick(ms);
  await settle();
}
const hide = (hidden: boolean) => {
  page.hidden = hidden;
  page.dispatchEvent(new Event('visibilitychange'));
};

beforeEach(() => {
  asked = [];
  log = {};
});
afterEach(settle);

test('the first look comes by itself once a start\'s first seconds are over, and not before', async () => {
  await gone(FIRST_LOOK_MS - 1);
  assert.deepEqual(asked, []);
  assert.equal(moduleFetched, false, 'not the module either: the page\'s own first reads go first');
  await gone(1);
  assert.deepEqual(asked, WHOLES);
  assert.equal(indexSync.rev, 5);
});

test('hidden a while and back is no start', async () => {
  hide(true);
  await gone(ANEW_AFTER_MS - 1);
  hide(false);
  await gone(FIRST_LOOK_MS);
  assert.deepEqual(asked, []);
});

test('hidden for hours and back is a start: its first seconds, then a look', async () => {
  hide(true);
  await gone(ANEW_AFTER_MS);
  hide(false);
  await gone(FIRST_LOOK_MS - 1);
  assert.deepEqual(asked, []);
  await gone(1);
  // An account's own goes by how long it has stood, and that is hours by now.
  assert.deepEqual(asked.filter((a) => a !== '/api/index/own'), UNTOLD);
});

test('the connection changing starts a look', async () => {
  connection.dispatchEvent(new Event('change'));
  await settle();
  assert.deepEqual(asked, UNTOLD);
});

test('reads moving back to the server start a look, and moving away does not', async () => {
  networkSaid(false);
  await settle();
  assert.deepEqual(asked, []);
  reached();
  await settle();
  assert.deepEqual(asked, UNTOLD);
});

test('the log\'s word on the library: it is told where the index stands, and moved it starts a look', async () => {
  await loadCapabilities();
  await settle();
  log = { index: { rev: 9, changed: true } };
  await readChangesNow();
  await settle();
  assert.deepEqual(asked, ['/api/index/changes?since=5', '/api/index/sounds/changes?since=5', ...UNTOLD]);
});

test('the log saying nothing moved starts none', async () => {
  log = { index: { rev: 5, changed: false } };
  await readChangesNow();
  await settle();
  assert.deepEqual(asked, []);
});

test('another account: its own date, its own first seconds, and its own whole', async () => {
  accountLearnt('bob');
  await settle();
  assert.equal(indexSync.rev, null);
  assert.equal(indexSync.at, null);
  await gone(FIRST_LOOK_MS - 1);
  assert.deepEqual(asked, []);
  await gone(1);
  assert.deepEqual(asked, WHOLES);
});

test('back to the first, its date is there again', async () => {
  accountLearnt('ann');
  await settle();
  assert.equal(indexSync.rev, 5);
  assert.equal(indexSync.covers, 5);
  assert.notEqual(indexSync.at, null);
  stopLooking();
});

test('a page opening on what stopped says so from the start', async () => {
  await writePart(whoOf('dan'), 'index', { rev: null, at: null, wants: 1, part: { whole: true, stopped: true, lost: 0, failed: 0, tried: null } });
  accountLearnt('dan');
  await settle();
  assert.equal(indexSync.shows, 'stopped');
});

test('nobody signed in looks no more', async () => {
  accountLearnt('bob');
  await settle();
  accountForgotten();
  await settle();
  await gone(FIRST_LOOK_MS);
  assert.deepEqual(asked, []);
  assert.equal(indexSync.at, null);
});
