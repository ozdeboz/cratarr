// Plays from: the account's pick goes to the server, and every page of the account reads what plays again.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const { setCopyPick } = await import('../../src/lib/api.ts');
const { copyPicked, library, listedAlbum } = await import('../../src/lib/albums.svelte.ts');

const OWN = 'cratarr//own/cy';

test('a pick is a PUT naming the library, its slashes in the body', async () => {
  let seen: { path: string; method?: string; body: unknown } | null = null;
  globalThis.fetch = (async (path: string, init?: RequestInit) => {
    seen = { path, method: init?.method, body: JSON.parse(init?.body as string) };
    return new Response(JSON.stringify({ library: OWN }), { headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch;
  assert.deepEqual(await setCopyPick('cratarr', 'al-1', OWN), { library: OWN });
  assert.deepEqual(seen, { path: '/api/albums/cratarr/al-1/copy-pick', method: 'PUT', body: { library: OWN } });
});

test('a pick shows on the list at once', () => {
  const copies = [{ library: 'music', format: 'FLAC', kbps: 900, plays: true, picked: false }, { library: 'in', format: 'MP3', kbps: 320, plays: false, picked: false }];
  library.view = { sources: [{ id: 'own', albums: [{ id: 'al-1', title: 'One', format: 'FLAC', bitrate_kbps: 900, copies }], library_info: [] }], trash_days: 7, music: null } as never;
  copyPicked('own', 'al-1', 'in');
  const a = listedAlbum('own', 'al-1')!.a;
  assert.deepEqual([a.format, a.copies?.[0].library, a.copies?.[0].picked], ['MP3', 'in', true]);
});

const read = (f: string) => readFileSync(new URL(`../../src/lib/${f}`, import.meta.url), 'utf8');

test('the Library and the open panel read themselves again when a pick is told', () => {
  assert.match(read('albums.svelte.ts'), /whenChanged\(\[[^\]]*'picks'/);
  assert.match(read('PlaylistView.svelte'), /whenChanged\(\[[^\]]*'picks'/);
});

test('the menu closes first, and Plays from needs an account and two copies', () => {
  const src = read('AlbumMenu.svelte');
  assert.match(src, /async function pick\(chosen: string\) \{\s+const \[src, id, already\][^\n]+\s+onclose\(\);/);
  assert.match(src, /listedHere && me\.name \? copiesToHear/);
});

test('the shelf shows each card as the switches stand', () => {
  assert.match(read('AlbumShelf.svelte'), /\.map\(\(a\) => heardAs\(a, s\.library_info, on\)\)/);
});
