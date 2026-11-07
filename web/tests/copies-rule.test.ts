// A record and its copies (lib/copiesRule.ts, docs/records-and-copies.md): a record shows when any copy is in a library
// switched on; its card has a dot for each library foreign to home, the first kept library; the copy that plays is the
// first on in the server's order, an Incoming one only when the server says so; a record of several copies has a line
// each in its panel.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import type { AlbumCopy, LibraryInfo } from '../src/lib/api.ts';

const { copyLines, copyPlaying, dotsOf, librariesOf, mayEdit, playFromEntries, recordOn } = await import('../src/lib/copiesRule.ts');

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
  size: null,
  tracks: 3,
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

// A card's dots (the owner, 2026-11-06). Home is the first kept library in the libraries' order: in INFO, Music.
test('a record only in home has no dot: home goes without saying', () => {
  assert.deepEqual(dotsOf(copies('music'), INFO, without()), []);
});

test('a record only in a second library has its dot, so it says where it lies', () => {
  assert.deepEqual(dotsOf(copies('more'), INFO, without()), [{ id: 'more', colour: '#more', name: 'MORE' }]);
});

test('a record in home and a second library shows both, in the list\'s order, each in its library\'s colour: a duplicate', () => {
  assert.deepEqual(dotsOf(copies('more', 'music'), INFO, without()), [
    { id: 'more', colour: '#more', name: 'MORE' },
    { id: 'music', colour: '#music', name: 'MUSIC' },
  ]);
});

test('home and Incoming read Incoming\'s dot alone: a copy waiting makes no duplicate', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'in'), INFO, without())), ['IN']);
});

test('a second library and Incoming read both', () => {
  assert.deepEqual(named(dotsOf(copies('more', 'in'), INFO, without())), ['MORE', 'IN']);
});

test('home and own music show both: own music is a kept library like any', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'own'), INFO, without())), ['MUSIC', 'OWN']);
});

test('a record only in own music has its dot', () => {
  assert.deepEqual(named(dotsOf(copies('own'), INFO, without())), ['OWN']);
});

test('every Incoming library holding a record has its dot, one each, with a kept copy or without', () => {
  const two = [...INFO, library('wait', 'incoming')];
  assert.deepEqual(named(dotsOf(copies('music', 'in', 'wait'), two, without())), ['IN', 'WAIT']);
  assert.deepEqual(named(dotsOf(copies('in', 'wait'), two, without())), ['IN', 'WAIT']);
  assert.deepEqual(named(dotsOf(copies('in'), INFO, without())), ['IN']);
});

test('a server of one library has no dots, whichever kind it is', () => {
  assert.deepEqual(dotsOf(copies('music'), [library('music', 'main')], without()), []);
  assert.deepEqual(dotsOf(copies('in'), [library('in', 'incoming')], without()), []);
});

test('no dots where the list has no libraries, or says nothing of them', () => {
  assert.deepEqual(dotsOf({}, INFO, without()), []);
  assert.deepEqual(dotsOf(copies('music'), undefined, without()), []);
  assert.deepEqual(dotsOf(copies('gone'), INFO, without()), []);
});

test('home is the first kept library in the libraries\' order, of either kind, and never an Incoming one listed first', () => {
  const ownFirst = [library('own', 'own'), library('music', 'main')];
  assert.deepEqual(dotsOf(copies('own'), ownFirst, without()), []);
  assert.deepEqual(named(dotsOf(copies('music'), ownFirst, without())), ['MUSIC']);
  const waitingFirst = [library('in', 'incoming'), library('music', 'main'), library('more', 'main')];
  assert.deepEqual(dotsOf(copies('music'), waitingFirst, without()), []);
  assert.deepEqual(named(dotsOf(copies('more'), waitingFirst, without())), ['MORE']);
});

test('a switch does not change what home means: with home off a record in a second library keeps its dot', () => {
  assert.deepEqual(named(dotsOf(copies('more'), INFO, without('music'))), ['MORE']);
  assert.deepEqual(named(dotsOf(copies('music', 'more'), INFO, without('music'))), ['MORE']);
});

test('how many libraries are on says nothing of a dot: one on alone says what it says among others', () => {
  const pair = [library('music', 'main'), library('more', 'main')];
  assert.deepEqual(named(dotsOf(copies('more'), pair, only('more'))), ['MORE']);
  assert.deepEqual(named(dotsOf(copies('in'), INFO, only('in'))), ['IN']);
  assert.deepEqual(dotsOf(copies('music'), INFO, only('music')), []);
});

test('a library with nothing in it changes nothing: home stays calm beside own music before its first use', () => {
  assert.deepEqual(dotsOf(copies('music'), INFO, only('music', 'own')), []);
});

test('a library with no colour has no dot, and the others keep theirs', () => {
  const plain = [library('music', 'main', null), library('in', 'incoming'), library('own', 'own')];
  assert.deepEqual(named(dotsOf(copies('music', 'own'), plain, without())), ['OWN']);
  assert.deepEqual(named(dotsOf(copies('music', 'in'), plain, without())), ['IN']);
  assert.deepEqual(named(dotsOf(copies('music', 'more'), [library('music', 'main'), library('more', 'main', null)], without())), ['MUSIC']);
});

test('a copy in a library switched off has no dot while another copy is on, and makes no duplicate', () => {
  assert.deepEqual(named(dotsOf(copies('more', 'own'), INFO, without('own'))), ['MORE']);
  assert.deepEqual(dotsOf(copies('music', 'own'), INFO, without('own')), []);
  assert.deepEqual(dotsOf(copies('music', 'in'), INFO, without('in')), []);
});

test('with the kept library off, the Incoming copy that shows the record has its dot', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'in'), INFO, without('music'))), ['IN']);
});

test('a card reached with all its libraries off names where it lies, home too', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'in'), INFO, without('music', 'in'))), ['MUSIC', 'IN']);
  assert.deepEqual(named(dotsOf(copies('music'), INFO, without('music'))), ['MUSIC']);
  assert.deepEqual(named(dotsOf({ library: 'in' }, INFO, without('in'))), ['IN']);
});

test('two folders of one library are one dot and one name, and of home no duplicate', () => {
  assert.deepEqual(named(dotsOf(copies('music', 'music', 'own'), INFO, without())), ['MUSIC', 'OWN']);
  assert.deepEqual(named(dotsOf(copies('in', 'in'), INFO, without())), ['IN']);
  assert.deepEqual(dotsOf(copies('music', 'music'), INFO, without()), []);
  assert.deepEqual(librariesOf(copies('music', 'music')), ['music']);
});

test('a list that names one library goes by that library', () => {
  assert.deepEqual(named(dotsOf({ library: 'in' }, INFO, without())), ['IN']);
  assert.deepEqual(named(dotsOf({ library: 'more' }, INFO, without())), ['MORE']);
  assert.deepEqual(dotsOf({ library: 'music' }, INFO, without()), []);
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

// A record's copies in its panel, a line each (the owner, 2026-11-06).
const MP3_AND_FLAC = { copies: [copy('music', { format: 'MP3', kbps: 320, size: 92e6, plays: true }), copy('in', { format: 'FLAC', size: 239e6 })] };
const FLAC_LINE = { name: 'IN', colour: '#in', quality: 'FLAC', size: '239 MB', plays: false, playFrom: { library: 'in', label: 'Play from IN' } };
const marked = (lines: { name: string; plays: boolean }[]) => lines.filter((l) => l.plays).map((l) => l.name);

test('a record of two copies has a line each, saying what each is, which plays, and what a tap on the other does', () => {
  assert.deepEqual(copyLines(MP3_AND_FLAC, INFO, without()), [{ name: 'MUSIC', colour: '#music', quality: 'MP3 320', size: '92 MB', plays: true, playFrom: null }, FLAC_LINE]);
});

test('a copy short of the record\'s tracks says how much of it is there, where a whole one says its size', () => {
  const a = { tracks: 10, copies: [copy('music', { plays: true, tracks: 10, size: 92_000_000 }), copy('in', { tracks: 3, size: 30_000_000 })] };
  assert.deepEqual(copyLines(a, INFO, without()).map((l) => l.size), ['92 MB', '3 of 10']);
});

test('an administrator may edit any record, and nobody else one in a main library or Incoming, a copy of their own beside it or not', () => {
  const info = [library('music', 'main'), library('in', 'incoming'), { ...library('mine', 'own'), mine: true }, library('yours', 'own')];
  assert.equal(mayEdit(copies('music'), info, true), true);
  assert.equal(mayEdit(copies('yours'), info, true), true);
  assert.equal(mayEdit(copies('music'), info, false), false);
  assert.equal(mayEdit(copies('mine', 'music'), info, false), false, 'owned by the administrators');
  assert.equal(mayEdit(copies('mine', 'in'), info, false), false);
  assert.equal(mayEdit({}, info, false), false, 'a list that names no copies');
});

test('a record only in people\'s own music is the owner\'s of the copy added first', () => {
  const info = [{ ...library('mine', 'own'), mine: true }, library('yours', 'own')];
  assert.equal(mayEdit(copies('mine'), info, false), true);
  assert.equal(mayEdit(copies('yours'), info, false), false, 'not another\'s');
  const first = { copies: [copy('yours', { added: 200 }), copy('mine', { added: 100 })] };
  assert.equal(mayEdit(first, info, false), true, 'mine came first');
  assert.equal(mayEdit({ copies: [copy('yours', { added: 100 }), copy('mine', { added: 200 })] }, info, false), false, 'theirs did');
});

test('lines keep the list\'s order, which is the order the copies would play, wherever the marked one stands in it', () => {
  assert.deepEqual(named(copyLines(copies('own', 'music', 'in'), INFO, without())), ['OWN', 'MUSIC', 'IN']);
  assert.deepEqual(named(copyLines(copies('in', 'music'), INFO, without())), ['IN', 'MUSIC']);
});

test('a record of one copy has no lines: its details say it', () => {
  assert.deepEqual(copyLines({ copies: [copy('music', { plays: true })] }, INFO, without()), []);
  assert.deepEqual(copyLines({ copies: [copy('in', { plays: true })] }, INFO, without()), []);
});

test('a list that names no copies has no lines', () => {
  assert.deepEqual(copyLines({ library: 'music' }, INFO, without()), []);
  assert.deepEqual(copyLines({}, INFO, without()), []);
  assert.deepEqual(copyLines({ copies: [] }, INFO, without()), []);
});

test('a copy in a library off has no line, and the others keep theirs', () => {
  assert.deepEqual(named(copyLines(copies('music', 'more', 'in'), INFO, without('more'))), ['MUSIC', 'IN']);
});

test('with every copy in a library off there are no lines', () => {
  assert.deepEqual(copyLines(copies('music', 'in'), INFO, without('music', 'in')), []);
});

test('the one copy a switch leaves on keeps its line, so a FLAC seen with the kept MP3 off reads FLAC', () => {
  assert.deepEqual(copyLines(MP3_AND_FLAC, INFO, without('music')), [FLAC_LINE]);
});

test('the mark goes by the switches as they stand, not by what the server said before one was thrown', () => {
  const a = { copies: [copy('music', { plays: true }), copy('more'), copy('own')] };
  assert.deepEqual(marked(copyLines(a, INFO, without('music'))), ['MORE']);
});

test('an Incoming copy is not marked beside a kept one, wherever it is listed', () => {
  assert.deepEqual(marked(copyLines({ copies: [copy('in'), copy('music')] }, INFO, without())), ['MUSIC']);
});

test('nothing is marked where nothing plays by itself: an Incoming copy with the kept library off', () => {
  assert.deepEqual(marked(copyLines(copies('music', 'in'), INFO, without('music'))), []);
});

test('of a record only in Incoming the copy the server says plays is marked', () => {
  const waiting = [...INFO, library('wait', 'incoming')];
  assert.deepEqual(marked(copyLines({ copies: [copy('in', { plays: true }), copy('wait')] }, waiting, without())), ['IN']);
});

test('the copy that plays has no tap: a tap would only be Play again', () => {
  assert.deepEqual(copyLines(MP3_AND_FLAC, INFO, without())[0].playFrom, null);
});

test('a line\'s tap is the menu\'s own entry, for just the copies that have one, however the switches stand', () => {
  const a = { copies: [copy('music', { plays: true }), copy('more'), copy('in')] };
  for (const lit of [without(), without('music'), without('more'), without('in'), without('music', 'more')]) {
    assert.deepEqual(
      copyLines(a, INFO, lit).flatMap((l) => l.playFrom ?? []),
      playFromEntries(a, INFO, lit),
    );
  }
});

test('of two folders of a library each has a line, and only the first a tap, as that is the one `from` names', () => {
  const a = { copies: [copy('music', { plays: true }), copy('in', { format: 'FLAC' }), copy('in', { format: 'MP3', kbps: 192 })] };
  const lines = copyLines(a, INFO, without());
  assert.deepEqual(
    lines.map((l) => l.quality),
    ['FLAC', 'FLAC', 'MP3 192'],
  );
  assert.deepEqual(
    lines.map((l) => l.playFrom?.library ?? null),
    [null, 'in', null],
  );
});

test('of two folders of the library that plays only the first is marked, and neither has a tap', () => {
  const lines = copyLines({ copies: [copy('music', { plays: true }), copy('music', { format: 'MP3', kbps: 320 })] }, INFO, without());
  assert.deepEqual(
    lines.map((l) => [l.quality, l.plays, l.playFrom]),
    [
      ['FLAC', true, null],
      ['MP3 320', false, null],
    ],
  );
});

test('a lossy copy says its bitrate, a lossless one its format alone, and one of no single format what a card says', () => {
  const a = { copies: [copy('music', { format: 'MP3', kbps: 320 }), copy('more', { format: 'FLAC', kbps: 916 }), copy('own', { format: 'OGG', kbps: null }), copy('in', { format: null, kbps: 128 })] };
  assert.deepEqual(
    copyLines(a, INFO, without()).map((l) => l.quality),
    ['MP3 320', 'FLAC', 'OGG', 'Mixed formats'],
  );
});

test('a line leaves out a size the list does not say', () => {
  const a = { copies: [copy('music', { size: null }), copy('more', { size: undefined }), copy('own', { size: 0 })] };
  assert.deepEqual(
    copyLines(a, INFO, without()).map((l) => l.size),
    [null, null, null],
  );
});

test('a line writes a size as the page writes sizes', () => {
  const a = { copies: [copy('music', { size: 92_400_000 }), copy('more', { size: 1_240_000_000 })] };
  assert.deepEqual(
    copyLines(a, INFO, without()).map((l) => l.size),
    ['92 MB', '1.2 GB'],
  );
});

test('every line has its library\'s colour, even where a card would show no dot for it', () => {
  const lit = only('music', 'in');
  assert.deepEqual(
    copyLines(copies('music', 'in'), INFO, lit).map((l) => l.colour),
    ['#music', '#in'],
  );
  assert.deepEqual(
    copyLines(copies('music', 'in'), [library('music', 'main', null), library('in', 'incoming')], lit).map((l) => l.colour),
    [null, '#in'],
  );
});

test('a copy in a library the list does not describe goes by its id, with no dot', () => {
  assert.deepEqual(
    copyLines(copies('gone', 'music'), INFO, without()).map((l) => [l.name, l.colour]),
    [
      ['gone', null],
      ['MUSIC', '#music'],
    ],
  );
});

// The panel's use of the lines, read from the source, as the view does not run here.
const view = readFileSync(new URL('../src/lib/PlaylistView.svelte', import.meta.url), 'utf8');

test("a line's tap goes through the record's own Play and names its copy, as the menu's entry does", () => {
  assert.match(view, /onclick=\{\(\) => playCopy\(from\)\}/);
  assert.match(view, /playListedAlbum\(listedHere\.s\.id, listedHere\.a, library\)/);
});

test("where lines show, the details say no copy's quality and carry no dots, and without lines they say them as ever", () => {
  assert.match(view, /const heard = albumCopies\.length \? \[\] : \[h\?\.format, h\?\.kbps \? `\$\{h\.kbps\} kb\/s` : null\];/);
  assert.match(view, /\[length \? clockText\(length\) : null, h\?\.year, \.\.\.heard\]/);
  assert.match(view, /const headDots = \$derived\(albumCopies\.length \? \[\] : albumDots\);/);
  assert.match(view, /\{#each headDots as dot \(dot\.id\)\}/);
});
