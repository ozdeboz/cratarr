// Reads in the app: what the app keeps, through its own address, read by the same index.wasm (lib/deviceIndex, the real one, the rules
// crate's own bytes, on a stand-in for the app's end). The page holds no copy of its own there.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';
import { databases } from '../harness/fakeIdb.ts';

const g = globalThis as Record<string, unknown>;
g.CratarrApp = { postMessage() {}, onmessage: null };
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });
localStorage.setItem('cratarr-who', '"ann"');

const vector = (name: string) => new Uint8Array(readFileSync(new URL(`../../../server/rules/tests/vectors/index/${name}`, import.meta.url)));
const wasm = readFileSync(new URL('../../src/lib/wasm/index.wasm', import.meta.url));

/** What the app answers at its address. */
let kept: (() => Response) | null = null;
const asked: string[] = [];
globalThis.fetch = (async (input: string) => {
  const url = String(input);
  if (url.includes('index.wasm')) return new Response(wasm);
  asked.push(url);
  if (url === '/__device/index') return kept ? kept() : new Response('', { status: 404 });
  return new Response('{}', { headers: { 'content-type': 'application/json' } });
}) as typeof fetch;

const { accountForgotten, accountLearnt } = await import('../../src/lib/account.ts');
const { closeReads, deviceAlbums, deviceRecord } = await import('../../src/lib/deviceIndex.svelte.ts');

const AT = 1_700_000_000_000;
const keeps = (name = 'whole.bin') => () => new Response(vector(name), { headers: { 'X-Cratarr-At': String(AT) } });

beforeEach(() => {
  closeReads();
  accountLearnt('ann');
  kept = keeps();
  asked.length = 0;
});

test('the list is read from what the app keeps, as of when it last stood where Cratarr does', async () => {
  const view = await deviceAlbums();
  assert.equal(view?.sources[0].albums.length, 3);
  assert.equal(view?.sources[0].as_of, AT / 1000);
  assert.deepEqual(asked, ['/__device/index']);
});

test('a record is read from it too, and it is fetched from the app once', async () => {
  const list = await deviceAlbums();
  const id = list!.sources[0].albums[0].id;
  const record = await deviceRecord('cratarr', id);
  assert.equal(record?.id, id);
  assert.deepEqual(asked, ['/__device/index']);
});

test('nothing kept by the app is no answer, and what it brings later is read next', async () => {
  kept = null;
  assert.equal(await deviceAlbums(), null);
  kept = keeps();
  assert.equal((await deviceAlbums())?.sources[0].albums.length, 3);
});

test('bytes of the app\'s that cannot be read answer nothing', async () => {
  kept = () => new Response(vector('whole.bin').slice(0, 100));
  assert.equal(await deviceAlbums(), null);
});

test('the app not answering at all is no answer', async () => {
  kept = () => {
    throw new TypeError('Failed to fetch');
  };
  assert.equal(await deviceAlbums(), null);
});

test('nobody signed in reads nothing of the app\'s', async () => {
  accountForgotten();
  assert.equal(await deviceAlbums(), null);
  assert.deepEqual(asked, []);
});

test('the page keeps no store of its own for it', async () => {
  await deviceAlbums();
  assert.deepEqual([...databases.keys()].filter((d) => d.startsWith('cratarr-index')), []);
});
