// The Want path (W12, lib/wantPath): every Want sends its record, else its release group and Bandcamp page, by the album's own
// names; whether it was asked for comes from the account's requests by record, else by group, never names; a Want out of reach
// says it needs Cratarr.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { askedFor, wantFailed, wantOf } = await import('../src/lib/wantPath.ts');

const GROUP = '3cceaac3-ebe4-464d-ad70-76c74b4ae9d4';
const PAGE = 'https://grouch.bandcamp.com/album/further';

test('every Want sends its record where it has one, its names beside it', () => {
  assert.deepEqual(wantOf({ record: 'al-88', release_group: GROUP, page: PAGE, artist: 'GROUCH', album: 'Further', year: 2011 }), { record: 'al-88', artist: 'GROUCH', album: 'Further' });
});

test("without one, its release group and its Bandcamp page, by the album's own names", () => {
  assert.deepEqual(wantOf({ record: null, release_group: GROUP, page: PAGE, artist: 'GROUCH', album: 'Further', year: 2011, cover: null }), {
    release_group: GROUP,
    page: PAGE,
    artist: 'GROUCH',
    album: 'Further',
    year: 2011,
  });
  assert.deepEqual(wantOf({ page: PAGE, artist: 'GROUCH', album: 'Further' }), { page: PAGE, artist: 'GROUCH', album: 'Further' });
});

test('Requested by an open request of its record, else of its group; never by names, nor by one closed', () => {
  const mine = (record: number | null, release_group: string | null, state: string, status = 'approved') => ({ record, release_group, status, at: 0, state: { state } as never, artist: 'GROUCH', album: 'Further' });
  assert.equal(askedFor([mine(88, null, 'wanted')], { record: 'al-88' }), true);
  assert.equal(askedFor([mine(87, GROUP, 'wanted')], { record: 'al-88', release_group: GROUP }), false, 'another record of the group');
  assert.equal(askedFor([mine(null, GROUP.toUpperCase(), 'waiting_for_approval', 'pending')], { release_group: GROUP }), true);
  assert.equal(askedFor([mine(88, GROUP, 'declined', 'declined')], { record: 'al-88' }), false);
  assert.equal(askedFor([mine(null, null, 'wanted')], { record: null, release_group: null }), false, 'its names alone');
  assert.equal(askedFor([{ record: 88, release_group: null, status: 'pending', at: 0 }], { record: 'al-88' }), true, 'one just made, by its status');
});

test('a Want out of reach says it needs Cratarr; else the server says why', () => {
  assert.equal(wantFailed('Cannot reach the Cratarr server', true), 'Wanting needs Cratarr.');
  assert.equal(wantFailed('Requesting albums is not on for this account', false), 'Requesting albums is not on for this account');
  assert.equal(wantFailed(null, false), 'Could not ask for it');
});
