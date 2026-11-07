// What shows as not here with Cratarr away (lib/away, the real one): the marks go by where reads come from, so a blip dims nothing, and
// what can still play is never dimmed. What plays goes by the first failed request all the same, as a hold does.
import { afterEach, beforeEach, mock, test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { accountLearnt, perAccount } = await import('../../src/lib/account.ts');
accountLearnt('ann');
const kept = (id: string, album: string) => ({ source: 'cratarr', id, name: id, artist: null, album: 'A record', album_id: album, duration_secs: 60, bytes: 1, at: 1 });
localStorage.setItem(perAccount('cratarr-offline-index-2'), JSON.stringify({ 'cratarr/tr-1': kept('tr-1', 'al-1'), 'cratarr/tr-2': kept('tr-2', 'al-1') }));
globalThis.fetch = (async () => new Response('', { status: 502 })) as typeof fetch;

const { albumAway, albumHere, albumsHere, away, rowsAway } = await import('../../src/lib/away.svelte.ts');
const { reads, readsNow } = await import('../../src/lib/deviceIndex.svelte.ts');
const { LOOK_AGAIN_MS } = await import('../../src/lib/deviceIndexRule.ts');
const { networkSaid, reached, unreached } = await import('../../src/lib/reach.svelte.ts');

beforeEach(() => mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 2_000_000_000 }));
afterEach(() => {
  reached();
  mock.timers.reset();
});

/** The second look comes back, as the first failed. */
async function settled() {
  unreached();
  mock.timers.tick(LOOK_AGAIN_MS);
  for (let i = 0; i < 10; i++) await new Promise((go) => setImmediate(go));
}

test('with Cratarr answering nothing is dimmed', () => {
  assert.equal(albumAway('cratarr', 'al-9'), false);
  assert.equal(rowsAway('cratarr', ['tr-9']), false);
});

test('a first failed request dims nothing, though what plays goes by it at once', () => {
  unreached();
  assert.equal(away(), true);
  assert.equal(albumAway('cratarr', 'al-9'), false);
  assert.equal(rowsAway('cratarr', ['tr-9']), false);
});

test('settled out of reach, what is not here is dimmed, and what is here is not', async () => {
  await settled();
  assert.equal(reads.from, 'device');
  assert.equal(albumAway('cratarr', 'al-9'), true);
  assert.equal(albumAway('cratarr', 'al-1'), false);
  assert.equal(rowsAway('cratarr', ['tr-9']), true);
  assert.equal(rowsAway('cratarr', ['tr-9', 'tr-1']), false, 'one of them plays');
});

test('no network at all needs no settling', () => {
  networkSaid(false);
  assert.equal(reads.from, 'device');
  assert.equal(albumAway('cratarr', 'al-9'), true);
});

test('Cratarr back, nothing is dimmed', async () => {
  await settled();
  reached();
  assert.equal(reads.from, 'server');
  assert.equal(readsNow().from, 'server');
  assert.equal(albumAway('cratarr', 'al-9'), false);
});

test('the device\'s own music is here, wherever reads come from', () => {
  networkSaid(false);
  assert.equal(albumAway('device', 'al-9'), false);
});

test('what is here is counted by record, and each record with something here is named once', () => {
  assert.equal(albumHere('cratarr', 'al-1'), 2);
  assert.equal(albumHere('cratarr', 'al-9'), 0);
  assert.deepEqual([...albumsHere()], ['al-1']);
});
