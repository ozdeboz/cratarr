// A kept list in this browser that cannot be read (lib/offline, the real one; the owner, 2026-11-07): it says so, is never written over,
// and starts again when asked, from the streams this browser holds, the music staying.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
/** The streams the cache holds, by address. */
const streams = ['/api/stream/cratarr/tr-1', '/api/stream/cratarr/tr-2', '/api/stream/cratarr/tr-3'];
g.caches = {
  open: async () => ({
    keys: async () => streams.map((p) => ({ url: `http://cratarr.test${p}` })),
    match: async () => new Response('sound', { headers: { 'content-length': '1234' } }),
    delete: async () => true,
  }),
};
/** What Cratarr was asked, and whether it answers; what happens while it describes the streams. */
const asked: string[] = [];
let answers = true;
let meanwhile: (() => void) | null = null;
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const path = String(input);
  asked.push(path);
  if (!answers) throw new TypeError('Failed to fetch');
  if (path === '/api/tracks/describe') {
    meanwhile?.();
    meanwhile = null;
    const { tracks } = JSON.parse(String(init!.body)) as { tracks: { source: string; id: string }[] };
    const known: Record<string, object> = { 'tr-1': { name: 'One', artist: 'Ann', album: 'A record', album_id: 'al-1', duration_secs: 60 } };
    const described = tracks.map((t) => ({ ...t, name: '', artist: null, album: null, album_id: null, duration_secs: null, addable: true, favorite: null, found: false, ...(known[t.id] ? { ...known[t.id], found: true } : {}) }));
    return new Response(JSON.stringify({ tracks: described }), { headers: { 'content-type': 'application/json' } });
  }
  return new Response('{}', { headers: { 'content-type': 'application/json' } });
}) as typeof fetch;

const { accountLearnt, perAccount } = await import('../../src/lib/account.ts');
accountLearnt('ann');
const LIST = perAccount('cratarr-offline-index-2');
localStorage.setItem(LIST, '{"cratarr/tr-1": {"source": "cratar');
// Another account's list here, which holds the third.
localStorage.setItem(LIST.replace('@ann', '@bob'), JSON.stringify({ 'cratarr/tr-3': { source: 'cratarr', id: 'tr-3' } }));
const { forgetOffline, keepOffline, offline, startOfflineAgain } = await import('../../src/lib/offline.svelte.ts');

test('a kept list that cannot be read says so', () => {
  assert.equal(offline.unreadable, true);
  assert.deepEqual(offline.kept, {});
});

test("Download… is not offered while it cannot be read: its entry says why, before anything that would count tracks coming", () => {
  const entry = readFileSync(new URL('../../src/lib/DownloadEntry.svelte', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
  const first = entry.indexOf('{#if pwa && offline.unreadable}');
  assert.ok(first > 0 && first < entry.indexOf('{:else if coming}'), 'the first thing the installed page is offered');
  assert.match(entry.slice(first), /^\{#if pwa && offline\.unreadable\}\s*(?:<!--[\s\S]*?-->\s*)?<MenuEntry icon="arrowDownToLine" label="Kept list can't be read" disabled /);
});

test('nothing is kept while it cannot be read, and nothing is written over it', async () => {
  await keepOffline([{ source: 'cratarr', id: 'tr-9', name: 'Nine', artist: null, album: null, album_id: null }]);
  assert.deepEqual(asked.filter((a) => a.startsWith('/api/offline')), [], 'Cratarr not asked to make anything');
  assert.equal(localStorage.getItem(LIST), '{"cratarr/tr-1": {"source": "cratar');
});

test('a removal while it cannot be read writes nothing over it', async () => {
  await forgetOffline([{ source: 'cratarr', id: 'tr-8' }]);
  assert.equal(localStorage.getItem(LIST), '{"cratarr/tr-1": {"source": "cratar');
});

test('another account coming while Cratarr describes the streams gets none of it, and the list stays as it was', async () => {
  meanwhile = () => accountLearnt('bob');
  await startOfflineAgain();
  assert.deepEqual(Object.keys(offline.kept), ['cratarr/tr-3'], "bob's own");
  accountLearnt('ann');
  assert.equal(offline.unreadable, true);
  assert.equal(localStorage.getItem(LIST), '{"cratarr/tr-1": {"source": "cratar');
});

test('with Cratarr out of reach, starting again fails and leaves it as it was', async () => {
  answers = false;
  await assert.rejects(startOfflineAgain());
  assert.equal(offline.unreadable, true);
  assert.equal(localStorage.getItem(LIST), '{"cratarr/tr-1": {"source": "cratar');
  answers = true;
});

test("started again, it lists what this browser holds that no one else's list does, as Cratarr describes it, and keeps it", async () => {
  await startOfflineAgain();
  assert.equal(offline.unreadable, false);
  assert.deepEqual(Object.keys(offline.kept).sort(), ['cratarr/tr-1', 'cratarr/tr-2']);
  const one = offline.kept['cratarr/tr-1'];
  assert.deepEqual([one.name, one.artist, one.album, one.album_id, one.duration_secs, one.bytes], ['One', 'Ann', 'A record', 'al-1', 60, 1234]);
  assert.equal(offline.kept['cratarr/tr-2'].name, 'tr-2', 'a track Cratarr no longer knows stays, by its id');
  assert.deepEqual(Object.keys(JSON.parse(localStorage.getItem(LIST)!)).sort(), ['cratarr/tr-1', 'cratarr/tr-2']);
});

test('a list that reads as something else than a list cannot be read either', async () => {
  localStorage.setItem(LIST.replace('@ann', '@cy'), '[]');
  accountLearnt('cy');
  assert.equal(offline.unreadable, true);
  accountLearnt('ann');
  assert.equal(offline.unreadable, false);
});
