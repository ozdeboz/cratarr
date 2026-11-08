// A crate's favourites toggle never set takes favourites in, everywhere (the owner, 2026-11-07): the page's word for a crate the
// Crates page does not list is the server's for one it does (lib/crates, the real one).
import { test } from 'node:test';
import assert from 'node:assert/strict';
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
globalThis.fetch = (async () => {
  throw new TypeError('Failed to fetch');
}) as typeof fetch;

const { crates, favouritesOf } = await import('../../src/lib/crates.svelte.ts');
type Listed = NonNullable<typeof crates.list>[number];

test("a crate the list does not hold takes favourites in: a word's, a Sounds like, Home's, any", () => {
  crates.list = [];
  for (const id of ['genre:Dub', 'like:tr-1', 'home:new', 'unlisted', 'second-chance']) assert.equal(favouritesOf(id), 'all', id);
});

test('one the list holds goes by what Cratarr said of it', () => {
  crates.list = [{ id: 'unlisted', favourites: 'none' } as Listed];
  assert.equal(favouritesOf('unlisted'), 'none');
  assert.equal(favouritesOf('home:new'), 'all');
});
