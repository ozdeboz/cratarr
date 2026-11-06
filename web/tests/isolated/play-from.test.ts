// Play from (docs/records-and-copies.md): a record's queue started from one of its copies, with nothing remembered. The
// real player on tests/harness/fakeStream, started through the record's real Play, so what a test reads is what the
// server was asked. AlbumMenu's wiring is read from its source, which says a line is there, not that it works.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { audio, fromRest, player, runFor, server, settle, track } from '../harness/fakeStream.ts';

const { enqueue, keptHere, local, playOnCommand, playQueue, seek, setRepeat, switchLocal, windUrl } = player;
const { accountForgotten, accountLearnt, perAccount } = await import('../../src/lib/account.ts');
const { library, playListedAlbum } = await import('../../src/lib/albums.svelte.ts');
const { heardAs } = await import('../../src/lib/copiesRule.ts');
const { endPreview, startPreview } = await import('../../src/lib/preview.svelte.ts');

/** A library's id is its folder, so it holds slashes, and here a space. */
const COPY = 'cratarr//new music/in';
const RECORD = { id: 'al-1', title: 'Both Shores' };

/** The record's tracks, as its view answers. */
let rows: string[] = [];
const streams = globalThis.fetch;
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  if (String(input) !== '/api/albums/cratarr/al-1') return streams(input, init);
  const lines = rows.map((id) => ({ id, name: id, file: id, artist: null, album: RECORD.title, duration_secs: server.lengths.get(id) }));
  return new Response(JSON.stringify({ source: 'cratarr', title: RECORD.title, genres: [], rows: lines }), { headers: { 'content-type': 'application/json' } });
}) as typeof fetch;

const record = (...tracks: ReturnType<typeof track>[]) => (rows = tracks.map((t) => t.id));

/** The record started by its Play; with `copy`, by Play from. */
async function play(copy?: string) {
  await playListedAlbum('cratarr', RECORD, copy);
  await settle();
}

const copyOf = (address: string) => new URL(address, 'http://cratarr.test').searchParams.get('from');
/** Each stream asked of the server: its track, from where, and the copy it named. */
const asked = () => server.asked.map((a, i) => [a.track, a.from, copyOf(server.addresses[i])]);
const queue = () => local.queue.map((t) => [t.id, t.from]);

beforeEach(async () => {
  await fromRest();
  library.view = null;
});

test('Play from starts the record as Play does, every track of its queue naming the copy', async () => {
  record(track('r1', 100), track('r2', 100));
  await play(COPY);
  assert.deepEqual(queue(), [['r1', COPY], ['r2', COPY]]);
  assert.equal(local.from, RECORD.title);
  assert.equal(local.index, 0);
  assert.equal(audio.heard()?.track, 'r1');
  assert.deepEqual(asked(), [['r1', 0, COPY]]);
});

test("a library's id reaches the stream whole: its slashes and its space URL-encoded", async () => {
  record(track('r1', 100));
  await play(COPY);
  assert.equal(server.addresses[0], '/api/stream/cratarr/r1?from=cratarr%2F%2Fnew+music%2Fin');
});

test('a plain Play names no copy', async () => {
  record(track('r1', 100));
  await play();
  assert.deepEqual(queue(), [['r1', undefined]]);
  assert.equal(server.addresses[0], '/api/stream/cratarr/r1');
});

test('nothing is remembered: the next Play is the best copy again', async () => {
  record(track('r1', 100));
  await play(COPY);
  await play();
  assert.deepEqual(queue(), [['r1', undefined]]);
  assert.deepEqual(asked().map(([, , copy]) => copy), [COPY, null]);
});

test('a seek past what is held asks for the same copy, and so does the track after it', async () => {
  record(track('r1', 200), track('r2', 100));
  await play(COPY);
  seek(150, false);
  await settle();
  await runFor(30);
  assert.deepEqual(asked(), [['r1', 0, COPY], ['r1', 150, COPY], ['r2', 0, COPY]]);
});

test('a track that breaks is asked for again from the same copy', async () => {
  server.plans.set('r1', [{ gives: 50, then: 'breaks' }]);
  record(track('r1', 200));
  await play(COPY);
  assert.deepEqual(asked(), [['r1', 0, COPY], ['r1', 50, COPY]]);
});

test("Repeat one's replay asks for the same copy", async () => {
  record(track('r1', 100));
  await play(COPY);
  setRepeat('one');
  await runFor(105);
  assert.equal(audio.heard()?.track, 'r1');
  assert.ok(server.addresses.length > 1, 'its start was asked for again');
  assert.deepEqual([...new Set(asked().map(([, , copy]) => copy))], [COPY]);
});

test("Repeat all's next round goes on naming the copy", async () => {
  record(track('r1', 40), track('r2', 40));
  await play(COPY);
  setRepeat('all');
  await runFor(50);
  assert.ok(local.queue.length > 2, 'the next round is there');
  assert.deepEqual([...new Set(local.queue.map((t) => t.from))], [COPY]);
});

test('a track added afterwards names no copy, and the record goes on naming its own after it', async () => {
  record(track('r1', 40), track('r2', 40));
  await play(COPY);
  enqueue([track('x', 40)], 'last');
  await runFor(60);
  assert.deepEqual(queue(), [['r1', COPY], ['x', undefined], ['r2', COPY]]);
  assert.deepEqual(asked(), [['r1', 0, COPY], ['x', 0, null], ['r2', 0, COPY]]);
});

test('what Play does to the queue before it, Play from does: what was added by hand stays, and the queue left is kept', async () => {
  record(track('r1', 40), track('r2', 40));
  playQueue([track('a', 40)]);
  await settle();
  enqueue([track('x', 40)], 'last');
  await play(COPY);
  assert.deepEqual(queue(), [['r1', COPY], ['x', undefined], ['r2', COPY]]);
  assert.equal(keptHere.list[0].tracks[0].id, 'a');
});

test('a queue left behind keeps its copy as kept for a reload, and taken up again plays it', async () => {
  record(track('r1', 100));
  await play(COPY);
  playQueue([track('other', 100)]);
  await settle();
  assert.equal(JSON.parse(localStorage.getItem(perAccount('cratarr-queues'))!)[0].tracks[0].from, COPY);
  switchLocal(keptHere.list[0].id);
  await settle();
  assert.deepEqual(asked().at(-1), ['r1', 0, COPY]);
});

test("a queue left behind is its account's on this device, its copy with it: hidden from another, and hers at her return", async () => {
  accountLearnt('ann');
  record(track('r1', 100));
  await play(COPY);
  playQueue([track('other', 100)]);
  await settle();
  const hers = keptHere.list.map((k) => k.id);
  assert.equal(keptHere.list[0].tracks[0].from, COPY);
  accountLearnt('bo');
  assert.deepEqual(keptHere.list, [], 'none of hers for him');
  accountForgotten();
  assert.equal(keptHere.list.some((k) => hers.includes(k.id)), false, 'nor for nobody');
  accountLearnt('ann');
  assert.deepEqual(keptHere.list.map((k) => k.id), hers);
  assert.equal(keptHere.list[0].tracks[0].from, COPY);
  assert.equal(localStorage.getItem('cratarr-queues'), null, 'and nothing under a name for no one');
  accountForgotten();
});

test('a queue Cratarr gives this page with a copy plays that copy', async () => {
  playOnCommand({ command: 'play', index: 0, tracks: [{ ...track('r1', 100), from: COPY }] });
  await settle();
  assert.deepEqual(asked(), [['r1', 0, COPY]]);
});

test("a wind asks for its track's copy, and one with none names none", () => {
  const wind = { ratio: 1, speed: 8, attackMs: 100, releaseMs: 100, pieceMs: 350, gainDb: -6 };
  assert.equal(copyOf(windUrl({ ...track('r1', 100), from: COPY }, 10, null, -1, 0, [], wind)), COPY);
  assert.equal(copyOf(windUrl(track('r1', 100), 10, null, -1, 0, [], wind)), null);
});

test("a queue row's cover held is heard from that queue's copy", async () => {
  const Stand = globalThis.Audio;
  let address = '';
  globalThis.Audio = class extends Stand {
    constructor(url: string) {
      super();
      address = url;
    }
  } as typeof Audio;
  record(track('r1', 90));
  await play(COPY);
  startPreview({ source: 'cratarr', id: 'r1', duration_secs: 90 });
  endPreview();
  globalThis.Audio = Stand;
  assert.equal(copyOf(address), COPY);
});

test('with a Play from queue running the card goes on saying what Play would give', async () => {
  const copies = [{ library: 'music', format: 'FLAC', kbps: 900, plays: true }, { library: COPY, format: 'MP3', kbps: 192, plays: false }];
  const listed = { ...RECORD, library: 'music', format: 'FLAC', bitrate_kbps: 900, copies };
  library.view = { sources: [{ id: 'cratarr', albums: [listed], library_info: [] }], trash_days: 7, music: null } as never;
  const before = library.view;
  record(track('r1', 100));
  await play(COPY);
  assert.equal(library.view, before, 'the list is not touched');
  const card = heardAs(listed as never, [], () => true) as typeof listed;
  assert.deepEqual([card.format, card.bitrate_kbps], ['FLAC', 900]);
});

const read = (f: string) => readFileSync(new URL(`../../src/lib/${f}`, import.meta.url), 'utf8');

test("Play from closes the menu first and goes through the record's own Play", () => {
  assert.match(read('AlbumMenu.svelte'), /onclick=\{\(\) => run\(\(\) => playListedAlbum\(source, a as ListedAlbum, p\.library\)\)\}/);
});

test('Play from is offered wherever the queue would go, and asks no account', () => {
  const line = read('AlbumMenu.svelte').match(/const playFrom = \$derived\(([^\n]+)\);/)?.[1] ?? '';
  assert.match(line, /^listedHere \? playFromEntries\(/);
  assert.doesNotMatch(line, /me\./);
});

test("this page's report tells Cratarr the copy of each track it holds", () => {
  assert.match(read('playOnTarget.svelte.ts'), /\.\.\.\(q\.from \? \{ from: q\.from \} : \{\}\)/);
});

test('the shelf shows each card as the switches stand', () => {
  assert.match(read('AlbumShelf.svelte'), /\.map\(\(a\) => heardAs\(a, s\.library_info, on\)\)/);
});
