// Editions of one album (F12.1; the owner: "One card, 2 editions"): a card standing for several editions says how many on its
// line (the artist page, search's Records, a label page); the album page lists each edition with its track count and copies,
// each opening its record. Display only: the records stay two (Q8). What the pages draw is read from the source.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { editionLines, editionsMark, withEditions } = await import('../src/lib/editions.ts');
const { rowLine } = await import('../src/lib/labelPage.ts');

const source = (file: string) => readFileSync(new URL(`../src/lib/${file}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const copy = (library: string, format: string | null) => ({ library, format }) as never;
const INFO = [
  { id: 'music', name: 'Music' },
  { id: 'incoming', name: 'Incoming' },
];
// Applescal's "A Slave's Commitment" (the finding): Music's 10-track MP3, and Incoming's 12-track FLAC of its release group.
const SLAVES = [
  { album: 'al-1296', tracks: 10, copies: [copy('music', 'MP3')] },
  { album: 'al-3095', tracks: 12, copies: [copy('incoming', 'FLAC')] },
];

test('a card standing for two editions is marked "2 editions" with its line piece; one edition, no mark', () => {
  assert.equal(editionsMark(2), '2 editions');
  assert.equal(editionsMark(3), '3 editions');
  for (const n of [undefined, null, 0, 1]) assert.equal(editionsMark(n), null, String(n));
  const card = source('AlbumCard.svelte');
  assert.match(card, /\{:else\}\{#if album\.year\}<span class="year">\{album\.year\}<\/span>\{\/if\}\{#if editionsMark\(album\.editions\)\}<span class="year">\{editionsMark\(album\.editions\)\}<\/span>\{\/if\}\{\/if\}/);
  assert.match(source('api.ts'), /\/\*\* How many editions of one album its card stands for \(F12\), its own among them; absent where one\. \*\/\s*editions\?: number;/);
});

test("search's Records and a label page's rows say it on their line", () => {
  assert.equal(withEditions('Applescal · 2012', 2), 'Applescal · 2012 · 2 editions');
  assert.equal(withEditions('Applescal · 2012', undefined), 'Applescal · 2012');
  assert.equal(withEditions(null, 2), '2 editions');
  assert.equal(rowLine({ title: "A Slave's Commitment", artist: 'Applescal', year: 2012, had: true, editions: 2 }), 'Applescal · 2012 · 2 editions');
  assert.equal(rowLine({ title: "A Slave's Commitment", artist: 'Applescal', year: 2012, had: true }), 'Applescal · 2012');
  const results = source('GlobalResults.svelte');
  assert.match(results, /\{@const line = withEditions\(it\.sub, it\.editions\)\}/);
  assert.match(results, /\{#snippet sub\(\)\}\{line\}\{#if it\.composers\?\.length\}<span class="why">\{line \? ' · ' : ''\}/);
});

test('the album page lists each edition with its track count and copies, the one it shows among them', () => {
  assert.deepEqual(editionLines(SLAVES, 'al-1296', INFO), [
    { album: 'al-1296', text: '10 tracks · Music MP3', here: true },
    { album: 'al-3095', text: '12 tracks · Incoming FLAC', here: false },
  ]);
  assert.deepEqual(editionLines(SLAVES, 'al-3095', INFO).map((l) => [l.album, l.here]), [['al-1296', false], ['al-3095', true]], "in the server's order from either page");
  const both = [{ album: 'al-7', tracks: 1, copies: [copy('music', 'MP3'), copy('incoming', null)] }, { album: 'al-8', tracks: 9, copies: [] }];
  assert.deepEqual(editionLines(both, 'al-7', INFO).map((l) => l.text), ['1 track · Music MP3, Incoming', '9 tracks']);
});

test('one edition or none: no list', () => {
  assert.deepEqual(editionLines(undefined, 'al-1296', INFO), []);
  assert.deepEqual(editionLines([SLAVES[0]], 'al-1296', INFO), []);
  assert.equal(editionLines(SLAVES, 'al-1296', INFO).length, 2);
});

test("the album page's editions: the copy lines' piece, each but its own opening its record", () => {
  const view = source('PlaylistView.svelte');
  assert.match(view, /const editionShown = \$derived\(album && albumHead \? editionLines\(albumHead\.editions, album\.id, library\.view\?\.sources\.find\(\(x\) => x\.id === album\.source\)\?\.library_info\) : \[\]\);/);
  assert.match(
    view,
    /\{#if editionShown\.length\}\s*<div class="details copies">\s*\{#each editionShown as e \(e\.album\)\}\s*\{#if e\.here\}<p class="copy"><span class="copy-name">\{e\.text\}<\/span><\/p>\{:else\}<button class="copy" onclick=\{\(\) => openEdition\(e\.album\)\}><span class="copy-name">\{e\.text\}<\/span><\/button>\{\/if\}\s*\{\/each\}\s*<\/div>\s*\{\/if\}/,
  );
  assert.match(view, /const openEdition = \(id: string\) => album && viewOver\(albumSubject\(album\.source, id\)\);/);
  assert.match(view, /'reading' \| 'sources' \| 'editions'>/);
  assert.match(view, /sources: r\.sources, editions: r\.editions \};/);
  assert.match(source('api.ts'), /editions\?: import\('\.\/editions'\)\.Edition\[\];/);
});
