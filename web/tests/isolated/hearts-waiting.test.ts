// A heart set away (lib/favourites, lib/api, lib/later, the real ones; the owner, 2026-11-08: "offline, what shows must be what will
// happen"): the change waits in the outbox, and what a read says meanwhile, Cratarr's last copy above all, gives way to it. It showed
// off again within a second, the favourite albums' last copy read over it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
Object.defineProperty(globalThis, 'navigator', { value: { onLine: false, serviceWorker: { addEventListener() {} } }, configurable: true });

/** Cratarr out of reach: a change is not sent, and a read is the worker's last copy, from before the heart. */
const kept = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json', 'X-Cratarr-Kept': '1' } });
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const path = String(input);
  if ((init?.method ?? 'GET') !== 'GET') throw new TypeError('no network');
  if (path === '/api/favorites/cratarr/albums') return kept(['al-2']);
  if (path === '/api/favorites/cratarr/artists') return kept([]);
  if (path === '/api/track/cratarr/tr-1/playlists') return kept({ addable: true, album_id: 'al-1', favorite: false, playlists: [], info: null });
  throw new TypeError('no network');
}) as typeof fetch;

const { accountLearnt } = await import('../../src/lib/account.ts');
const { setAlbumFavorite, setArtistFavorite, setTrackFavorite, trackState, getFavoriteArtists } = await import('../../src/lib/api.ts');
const { isFavorite, loadAlbumFavorites, setFavorite } = await import('../../src/lib/favourites.svelte.ts');
const { later } = await import('../../src/lib/later.svelte.ts');

accountLearnt('ann');

test('an album’s heart set away stays on, though the last copy of the favourite albums was made before it', async () => {
  const r = await setAlbumFavorite('cratarr', 'al-1', true);
  setFavorite('cratarr', 'al-1', r.favorite);
  assert.equal(later.list.length, 1, 'it waits to be sent');
  assert.equal(await loadAlbumFavorites('cratarr', ['al-1']), true);
  assert.equal(isFavorite('cratarr', 'al-1'), true, 'on, as it will be');
  assert.equal(isFavorite('cratarr', 'al-2'), true, 'the rest as the copy says');
});

test('one taken off away stays off', async () => {
  await setAlbumFavorite('cratarr', 'al-2', false);
  await loadAlbumFavorites('cratarr', ['al-2']);
  assert.equal(isFavorite('cratarr', 'al-2'), false);
});

test('a track’s heart set away stays on in what its state says', async () => {
  await setTrackFavorite('cratarr', 'tr-1', true);
  assert.equal((await trackState('cratarr', 'tr-1')).favorite, true);
});

test('an artist’s heart set away is among the favourite artists', async () => {
  await setArtistFavorite('cratarr', 'Ann & Bo', true);
  assert.deepEqual(await getFavoriteArtists('cratarr'), ['Ann & Bo']);
});
