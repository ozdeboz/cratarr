// The door (lib/api's `read`, the real one): a read run both ways on the same data. The index is what the rules crate wrote of the
// small library in server/rules/tests/vectors/index, read by the real index.wasm; Cratarr here answers that library as its lists and a
// record's view say it, field by field. So a page cannot tell them apart, but for what `LEFT_OUT` names: what an index does not hold.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';
import { databases } from '../harness/fakeIdb.ts';

const { accountForgotten, accountLearnt, whoOf } = await import('../../src/lib/account.ts');
const { albumView, artistView, getAlbums, read, search, UNREACHED } = await import('../../src/lib/api.ts');
const { library, readLibrary } = await import('../../src/lib/albums.svelte.ts');
const { closeReads, readsNow } = await import('../../src/lib/deviceIndex.svelte.ts');
const { writePart } = await import('../../src/lib/indexStore.ts');
const { networkSaid, reach, reached, unreached } = await import('../../src/lib/reach.svelte.ts');

const vector = (name: string) => new Uint8Array(readFileSync(new URL(`../../../server/rules/tests/vectors/index/${name}`, import.meta.url)));
const wasm = readFileSync(new URL('../../src/lib/wasm/index.wasm', import.meta.url));

const MUSIC = 'cratarr//music';
const INCOMING = 'cratarr//in coming';
const OWN = 'own/ann';

/** One copy, as a list says it. */
const copy = (library: string, format: string, kbps: number, bits: number | null, size: number, added: number, plays: boolean, tracks: number) => ({
  library,
  format,
  kbps,
  bits,
  rate: bits === null ? null : 44100,
  deletable: false,
  keepable: false,
  keep_to: null,
  lacks: [],
  plays,
  added,
  size,
  tracks,
  job: null,
});

/** What the library says, as `GET /api/albums` says it for the account `ann`, beside what only Cratarr can say. */
const albums = () => ({
  sources: [
    {
      id: 'cratarr',
      label: 'Cratarr',
      supported: true,
      error: null,
      as_of: 1234,
      libraries: [MUSIC, INCOMING, OWN],
      library_info: [
        { id: MUSIC, name: 'Music', kind: 'main', fetcher: null, owner: null, mine: false, colour: '#aa3322', quality: 'none', seen: true },
        { id: INCOMING, name: 'Incoming', kind: 'incoming', fetcher: null, owner: null, mine: false, colour: '#22aa33', quality: 'none', seen: true },
        { id: OWN, name: 'Your music', kind: 'own', fetcher: null, owner: 'ann', mine: true, colour: '#3322aa', quality: 'none', seen: true },
      ],
      verdicts: {},
      lacks: {},
      waiting: {},
      deletable: [],
      keepable: [],
      keep_to: {},
      albums: [
        {
          id: 'al-10',
          title: 'First Light',
          artist: 'Ana Bell',
          year: 2001,
          added: 1001,
          path: `${MUSIC}/Ana Bell/First Light`,
          tracks: 2,
          format: 'FLAC',
          bitrate_kbps: 900,
          duration_secs: 380.5,
          tags: [],
          copies: [copy(MUSIC, 'FLAC', 900, 16, 5000, 1001, true, 2), copy(INCOMING, 'MP3', 320, null, 2000, 1002, false, 2)],
          type: 'album',
          secondary: ['live'],
        },
        {
          id: 'al-11',
          title: 'Second Wind',
          artist: 'Ana Bell & The Quiet',
          year: 2003,
          added: 1002,
          path: `${MUSIC}/Ana Bell/Second Wind`,
          tracks: 2,
          format: 'FLAC',
          bitrate_kbps: 700,
          duration_secs: null,
          tags: [],
          copies: [copy(MUSIC, 'FLAC', 700, 24, 3000, 1001, true, 2)],
          artists: ['Ana Bell', 'The Quiet'],
          type: 'ep',
          secondary: [],
          whole_tracks: 3,
        },
        {
          id: 'al-12',
          title: 'Own Thing',
          artist: 'Mo',
          year: null,
          added: null,
          path: `${OWN}/Mo/Own Thing`,
          tracks: 1,
          format: 'OPUS',
          bitrate_kbps: 128,
          duration_secs: 60,
          tags: [],
          copies: [copy(OWN, 'OPUS', 128, null, 900, 1003, true, 1)],
          type: null,
          secondary: [],
        },
      ],
    },
  ],
  trash_days: 14,
  music: 'Music',
});

/** What a record's view says of First Light, with what only Cratarr can say. */
const firstLight = () => ({
  source: 'cratarr',
  id: 'al-10',
  title: 'First Light',
  artist: 'Ana Bell',
  year: 2001,
  genres: ['Jazz', 'Soul'],
  format: 'FLAC',
  bitrate_kbps: 900,
  added: 1001,
  played: 1700000000,
  plays: 4,
  words: { kinds: [], tracks: [] },
  rows: [
    { entry: 'Ana Bell/First Light/01.flac', file: '01.flac', id: 'tr-100', name: 'Dawn', artist: 'Ana Bell', album: 'First Light', duration_secs: 200.5, missing: false, has_versions: true },
    { entry: 'Ana Bell/First Light/02.flac', file: '02.flac', id: 'tr-101', name: 'Noon', artist: 'Ana Bell', album: 'First Light', duration_secs: 180, missing: false },
  ],
});

/** What an index does not hold, and so what a read from it leaves out or says nothing of: a record's `plays`, a count, not a copy's. */
const LEFT_OUT = new Set(['as_of', 'trash_days', 'played', 'words', 'has_versions']);
const held = (v: unknown): unknown =>
  Array.isArray(v)
    ? v.map(held)
    : v && typeof v === 'object'
      ? Object.fromEntries(Object.entries(v).filter(([k, x]) => !LEFT_OUT.has(k) && !(k === 'plays' && typeof x === 'number')).map(([k, x]) => [k, held(x)]))
      : v;

/** Every request: what Cratarr was asked, and whether the file was fetched. */
let asked: string[] = [];
/** Nothing answers while it is out of reach. */
let answers = true;
/** The service worker answers the album list from its copy, at once. */
let copyShown = false;
globalThis.fetch = (async (input: string) => {
  const url = String(input);
  if (url.includes('index.wasm')) return new Response(wasm);
  asked.push(url);
  if (copyShown) return new Response(JSON.stringify(albums()), { headers: { 'content-type': 'application/json', 'X-Cratarr-Copy': '1' } });
  if (!answers) throw new TypeError('Failed to fetch');
  const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json' } });
  return url.startsWith('/api/albums/cratarr/al-10') ? json(firstLight()) : json(albums());
}) as typeof fetch;

/** Away, with nothing to settle. */
const noNetwork = () => {
  answers = false;
  networkSaid(false);
};

const keepIndex = (bytes: Uint8Array, who = 'ann') =>
  writePart(whoOf(who), 'index', { rev: 5, at: 1_234_000, wants: null, part: { whole: false, stopped: false, lost: 0, failed: 0, tried: null } }, bytes);

beforeEach(async () => {
  reached();
  answers = true;
  asked = [];
  closeReads();
  accountLearnt('ann');
  await keepIndex(vector('whole.bin'));
});

test('the Library\'s list reads the same both ways, field by field, but for what an index does not hold', async () => {
  const fromCratarr = await getAlbums();
  assert.equal(readsNow().from, 'server');
  noNetwork();
  const fromHere = await getAlbums();
  assert.equal(readsNow().from, 'device');
  assert.deepEqual(asked.filter((a) => a.startsWith('/api/albums')), ['/api/albums'], 'the device answered: nothing was asked');
  assert.deepEqual(held(fromHere), held(fromCratarr));
});

test('a record\'s view reads the same both ways, its tracks in the order they play', async () => {
  const fromCratarr = await albumView('cratarr', 'al-10');
  noNetwork();
  assert.deepEqual(held(await albumView('cratarr', 'al-10')), held(fromCratarr));
});

test('the album list\'s copy shown at once says nothing of Cratarr: it undoes no look that found it gone', async () => {
  unreached();
  copyShown = true;
  await getAlbums();
  copyShown = false;
  assert.equal(reach.away, true);
});

test('the list as of when the index last stood where Cratarr does', async () => {
  noNetwork();
  assert.equal((await getAlbums()).sources[0].as_of, 1234);
});

test('with Cratarr answering the device is not touched: the file is not even fetched', async () => {
  let opened = 0;
  const fetching = globalThis.fetch;
  globalThis.fetch = ((input: string) => {
    if (String(input).includes('index.wasm')) opened++;
    return fetching(input);
  }) as typeof fetch;
  await getAlbums();
  await albumView('cratarr', 'al-10');
  globalThis.fetch = fetching;
  assert.equal(opened, 0);
});

test('a read with no device answer goes where any read goes', async () => {
  noNetwork();
  assert.equal(await read(async () => 'its last copy', async () => null), 'its last copy');
});

test('a device answer that fails is no answer', async () => {
  noNetwork();
  const broken = async () => {
    throw new Error('no store');
  };
  assert.equal(await read(async () => 'its last copy', broken), 'its last copy');
});

test('a read from the device goes to no server', async () => {
  noNetwork();
  const server = async () => assert.fail('asked');
  assert.equal(await read(server, async () => 'here'), 'here');
});

test('a record the index does not hold ends there too, and so does an id that is none of its', async () => {
  noNetwork();
  await assert.rejects(albumView('cratarr', 'al-99'), (e: Error) => e.message === UNREACHED);
  await assert.rejects(albumView('cratarr', 'old-hash'), (e: Error) => e.message === UNREACHED);
});

test('another server\'s album is none of the index\'s', async () => {
  noNetwork();
  await assert.rejects(albumView('jellyfin', 'al-10'), (e: Error) => e.message === UNREACHED);
  await assert.rejects(getAlbums('jellyfin'), (e: Error) => e.message === UNREACHED);
});

test('nothing kept ends there too', async () => {
  accountLearnt('bob');
  noNetwork();
  await assert.rejects(getAlbums(), (e: Error) => e.message === UNREACHED);
});

test('nothing kept is no answer for good: what a sync brings is read next', async () => {
  noNetwork();
  accountLearnt('carol');
  await assert.rejects(getAlbums(), (e: Error) => e.message === UNREACHED);
  await keepIndex(vector('whole.bin'), 'carol');
  assert.equal((await getAlbums()).sources[0].albums.length, 3);
});

test('nobody reads anyone\'s, and one account\'s is not another\'s', async () => {
  noNetwork();
  assert.equal((await getAlbums()).sources[0].albums.length, 3);
  accountForgotten();
  await assert.rejects(getAlbums(), (e: Error) => e.message === UNREACHED);
  assert.equal(databases.has(`cratarr-index${whoOf(null)}`), false, 'nobody has no store');
  accountLearnt('bob');
  await assert.rejects(getAlbums(), (e: Error) => e.message === UNREACHED);
  accountLearnt('ann');
  assert.equal((await getAlbums()).sources[0].albums.length, 3);
});

test('a read under way as the account changes answers nothing of the one before\'s', async () => {
  noNetwork();
  const asking = getAlbums();
  accountLearnt('bob');
  await assert.rejects(asking, (e: Error) => e.message === UNREACHED);
});

test('another\'s own music keeps its name, and one\'s own is called so', async () => {
  await keepIndex(vector('whole.bin'), 'bob');
  accountLearnt('bob');
  noNetwork();
  const info = (await getAlbums()).sources[0].library_info.find((l) => l.id === OWN);
  assert.equal(info?.name, 'Ann\'s music');
  assert.equal(info?.mine, false);
});

test('bytes that cannot be read answer nothing, and the read goes on to the line', async () => {
  const torn = vector('whole.bin').slice(0, 100);
  await keepIndex(torn);
  noNetwork();
  await assert.rejects(getAlbums(), (e: Error) => e.message === UNREACHED);
});

test('the Library\'s list read before is read again when reads move, each way', async () => {
  await readLibrary(false);
  assert.equal(library.view?.sources[0].as_of, 1234);
  const gone = library.view;
  noNetwork();
  // Told as the reads moved, and read from the device: Cratarr was not asked.
  asked = [];
  for (let i = 0; i < 40 && library.view === gone; i++) await new Promise((go) => setTimeout(go, 5));
  assert.deepEqual(asked.filter((a) => a.startsWith('/api/albums')), []);
  assert.equal(library.view?.sources[0].albums[0].copies?.[0].plays, true);
  assert.equal(library.view?.trash_days, 0, 'the device\'s, not Cratarr\'s');
  answers = true;
  reached();
  for (let i = 0; i < 40 && library.view?.trash_days === 0; i++) await new Promise((go) => setTimeout(go, 5));
  assert.equal(library.view?.trash_days, 14);
});

test('moved back to the server, the index open for reads is let go: the next read away opens what is kept now', async () => {
  noNetwork();
  assert.equal((await getAlbums()).sources[0].albums.length, 3);
  reached();
  answers = true;
  await keepIndex(vector('after.bin'));
  noNetwork();
  assert.equal((await getAlbums()).sources[0].albums.length, 4);
});

const EVERY = ['artist', 'album', 'track', 'work', 'playlist', 'crate', 'genre', 'missing', 'spotify'] as const;
const lookedFor = () => asked.filter((a) => a.startsWith('/api/search') || a.startsWith('/api/artists'));

test('search with Cratarr away finds the library in the index, and the other tiers are left out with no notice', async () => {
  noNetwork();
  const found = await search('dawn', [...EVERY], 5, 0);
  assert.deepEqual(Object.keys(found.groups).sort(), ['album', 'artist', 'track']);
  assert.deepEqual(found.groups.track!.items.map((t) => t.title), ['Dawn']);
  assert.equal(found.groups.track!.items[0].track, 'tr-100');
  assert.deepEqual(found.top.map((t) => t.title), ['Dawn']);
  assert.deepEqual(lookedFor(), []);
});

test('asked only for what an index does not hold, a search finds nothing, and that is no failure', async () => {
  noNetwork();
  assert.deepEqual(await search('dawn', ['genre', 'missing'], 5, 0), { q: 'dawn', groups: {}, top: [] });
});

test('a library switched off takes its records out of what is found', async () => {
  noNetwork();
  const found = await search('first', ['album'], 5, 0, ['cratarr//music', 'cratarr//in coming']);
  assert.deepEqual(found.groups.album!.items, []);
});

test('Show all goes on from where it was, and knows how many there are', async () => {
  noNetwork();
  const [first, next] = [await search('ana', ['track'], 1, 0), await search('ana', ['track'], 1, 1)];
  assert.equal(next.groups.track!.total, 5);
  assert.equal(next.groups.track!.items.length, 1);
  assert.notEqual(next.groups.track!.items[0].track, first.groups.track!.items[0].track);
  assert.deepEqual(next.top, []);
});

test('with no index to ask, a search ends in the one line', async () => {
  accountLearnt('someone with none');
  noNetwork();
  await assert.rejects(search('dawn'), { message: UNREACHED });
});

test('with Cratarr answering, search is its own, and the index is not touched', async () => {
  await search('dawn', ['track'], 5, 0).catch(() => {});
  assert.deepEqual(lookedFor(), ['/api/search?q=dawn&kinds=track&limit=5']);
});

test("an artist's panel with Cratarr away reads from the index", async () => {
  noNetwork();
  const them = await artistView('cratarr', 'Ana Bell');
  assert.deepEqual(them.albums.map((a) => a.title), ['Second Wind', 'First Light']);
  assert.ok(them.rows.every((r) => r.album_id));
  assert.deepEqual(lookedFor(), []);
});

test("an artist the index does not hold, and another server's, end in the one line", async () => {
  noNetwork();
  await assert.rejects(artistView('cratarr', 'Nobody'), { message: UNREACHED });
  await assert.rejects(artistView('jellyfin', 'Ana Bell'), { message: UNREACHED });
});
