// A record and its copies (lib/copiesRule.ts, docs/records-and-copies.md): a record shows when any copy is in a library
// switched on; its card has a dot per copy under the rule of two or more, an Incoming copy none beside a kept one; the
// copy that plays is the first on in the server's order, an Incoming one only when the server says so.
import { test } from 'node:test';
import assert from 'node:assert/strict';

import type { AlbumCopy, LibraryInfo } from '../src/lib/api.ts';

const { copyPlaying, dotsOf, librariesOf, recordOn } = await import('../src/lib/copiesRule.ts');

const library = (id: string, kind: LibraryInfo['kind'], colour: string | null = `#${id}`) => ({ id, name: id.toUpperCase(), kind, colour }) as LibraryInfo;
const INFO = [library('music', 'main'), library('more', 'main'), library('in', 'incoming'), library('own', 'own')];

const copy = (id: string, more: Partial<AlbumCopy> = {}): AlbumCopy => ({
  library: id,
  format: 'FLAC',
  kbps: null,
  bits: 16,
  rate: 44100,
  deletable: false,
  keepable: false,
  keep_to: null,
  lacks: [],
  plays: false,
  added: null,
  job: null,
  ...more,
});
const copies = (...ids: string[]) => ({ copies: ids.map((id) => copy(id)) });

/** Every library on but the named ones. */
const without = (...off: string[]) => (id: string) => !off.includes(id);
const only = (...on: string[]) => (id: string) => on.includes(id);
const named = (dots: { name: string }[]) => dots.map((d) => d.name);

test('a record shows while any of its copies is in a library switched on', () => {
  assert.equal(recordOn(copies('music', 'own'), without('music')), true);
  assert.equal(recordOn(copies('music', 'own'), without('own')), true);
  assert.equal(recordOn(copies('music', 'own'), without('music', 'own')), false);
});

test('an Incoming copy alone on still shows its record: the Library is where it is found', () => {
  assert.equal(recordOn(copies('music', 'in'), only('in')), true);
});

test('a record with no copies is in no library, and a list with no libraries always shows', () => {
  assert.equal(recordOn({ copies: [] }, only('music')), false);
  assert.equal(recordOn({}, only()), true);
});

test('a list that names one library goes by it, and copies win where both are sent', () => {
  assert.equal(recordOn({ library: 'music' }, only('music')), true);
  assert.equal(recordOn({ library: 'music' }, only('own')), false);
  assert.equal(recordOn({ library: 'music', ...copies('music', 'own') }, only('own')), true);
  assert.deepEqual(librariesOf({ library: 'music', ...copies('own', 'music') }), ['own', 'music']);
});

test('a card has a dot per copy, in the list\'s order, each in its library\'s colour', () => {
  assert.deepEqual(dotsOf(copies('own', 'music'), INFO, without()), [
    { id: 'own', colour: '#own', name: 'OWN' },
    { id: 'music', colour: '#music', name: 'MUSIC' },
  ]);
});

test('no dots while fewer than two libraries are on, however many hold the record', () => {
  assert.deepEqual(dotsOf(copies('music', 'own'), INFO, only('music')), []);
  assert.deepEqual(dotsOf({ library: 'music' }, INFO, only('music')), []);
});

test('a library with no records yet counts towards two: own music before its first use', () => {
  assert.deepEqual(named(dotsOf(copies('music'), INFO, only('music', 'own'))), ['MUSIC']);
});

test('no dots where the list has no libraries, or says nothing of them', () => {
  assert.deepEqual(dotsOf({}, INFO, without()), []);
  assert.deepEqual(dotsOf(copies('music'), undefined, without()), []);
  assert.deepEqual(dotsOf(copies('gone'), INFO, without()), []);
});

test('a library with no colour has no dot, and its copy still keeps Incoming\'s off the card', () => {
  const plain = [library('music', 'main', null), library('in', 'incoming'), library('own', 'own')];
  assert.deepEqual(named(dotsOf(copies('music', 'own'), plain, without())), ['OWN']);
  assert.deepEqual(dotsOf(copies('music', 'in'), plain, without()), []);
});

test('an Incoming copy has no dot beside a kept copy, and its own on a record only in Incoming', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'in'), INFO, without())), ['MUSIC']);
  assert.deepEqual(named(dotsOf(copies('in', 'own'), INFO, without())), ['OWN']);
  assert.deepEqual(named(dotsOf(copies('in'), INFO, without())), ['IN']);
});

test('a copy in a library switched off has no dot while another copy is on', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'own'), INFO, without('own'))), ['MUSIC']);
});

test('with the kept library off, the Incoming copy that shows the record has its dot', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'in'), INFO, without('music'))), ['IN']);
});

test('a card reached with all its libraries off names where it lies', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'in'), INFO, without('music', 'in'))), ['MUSIC']);
  assert.deepEqual(named(dotsOf({ library: 'in' }, INFO, without('in'))), ['IN']);
});

test('two folders of one library are one dot and one name', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'music', 'own'), INFO, without())), ['MUSIC', 'OWN']);
  assert.deepEqual(librariesOf(copies('music', 'music')), ['music']);
});

test('a list that names one library has that library\'s dot', () => {
  assert.deepEqual(named(dotsOf({ library: 'in' }, INFO, without())), ['IN']);
});

test('the copy that plays is the first on in the server\'s order, even at a lower quality', () => {
  const a = { copies: [copy('own', { format: 'MP3', plays: true }), copy('music')] };
  assert.equal(copyPlaying(a, INFO, without())?.library, 'own');
});

test('a switch the server has not heard of yet moves what plays to the next copy on', () => {
  const a = { copies: [copy('music', { plays: true }), copy('more')] };
  assert.equal(copyPlaying(a, INFO, without('music'))?.library, 'more');
});

test('an Incoming copy is passed over for the kept copy, wherever it is listed', () => {
  assert.equal(copyPlaying({ copies: [copy('in'), copy('music')] }, INFO, without())?.library, 'music');
});

test('an Incoming copy better than the kept one still does not play by itself: nobody has checked it', () => {
  const a = { copies: [copy('in', { format: 'FLAC' }), copy('music', { format: 'MP3', kbps: 128 })] };
  assert.equal(copyPlaying(a, INFO, without())?.library, 'music');
});

test('with the kept library off an Incoming copy does not play by itself', () => {
  assert.equal(copyPlaying(copies('music', 'in'), INFO, without('music')), null);
});

test('a record only in Incoming plays its copy, the server saying so', () => {
  assert.equal(copyPlaying({ copies: [copy('in', { plays: true })] }, INFO, without())?.library, 'in');
});

test('nothing plays with every copy in a library off', () => {
  assert.equal(copyPlaying(copies('music', 'own'), INFO, without('music', 'own')), null);
});

test('a list that names no copies has no copy that plays', () => {
  assert.equal(copyPlaying({ library: 'music' }, INFO, without()), null);
  assert.equal(copyPlaying({}, INFO, without()), null);
  assert.equal(copyPlaying({ copies: [] }, INFO, without()), null);
});
