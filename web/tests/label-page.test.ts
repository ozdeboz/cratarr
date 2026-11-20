// A label's page (W11, C56; lib/labelPage): its subject in the panel, a record's label chip, a row's line, what "More" reads on,
// Discogs credited where its rows came from it, the page's picture, and a row opened by its record or its references with the label.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { fromDiscogs, labelChip, labelCount, labelCovers, labelOf, labelRows, labelSubject, moreToRead, rowLine, rowOpenable } = await import('../src/lib/labelPage.ts');
const { sectionPlace } = await import('../src/lib/recordNotHad.ts');
const { when } = await import('../src/lib/settings/when.ts');
const { howToOpen } = await import('../src/lib/openRecord.ts');

const row = (r: Record<string, unknown> = {}) => ({ title: 'Selected Ambient Works 85-92', artist: 'Aphex Twin', had: false, ...r });

test("a label's subject, and back; nothing else is a label", () => {
  assert.equal(labelSubject('la-5'), 'label:la-5');
  assert.equal(labelOf(labelSubject('la-5')), 'la-5');
  for (const other of ['label:Warp', 'label:', 'album:cratarr/al-5', 'crate:label:Warp', null]) assert.equal(labelOf(other), null, String(other));
});

test("a record's label chip names its catalogue number where it has one", () => {
  assert.equal(labelChip({ id: 'la-5', name: 'Warp', catno: 'WARP123' }), 'Warp · WARP123');
  assert.equal(labelChip({ id: 'la-5', name: 'Warp' }), 'Warp');
  assert.equal(labelChip({ id: 'la-5', name: 'Warp', catno: 'none' }), 'Warp');
});

test("a row says its artist, year, catalogue number, and its type where it is not an album", () => {
  assert.equal(rowLine(row({ year: 1992, catno: 'AMB 3922', kind: 'Album' })), 'Aphex Twin · 1992 · AMB 3922');
  assert.equal(rowLine(row({ kind: 'EP' })), 'Aphex Twin · EP');
});

test('More reads on each source whose list does, MusicBrainz first', () => {
  assert.deepEqual(moreToRead({ more: { musicbrainz: true, discogs: true } }), ['musicbrainz', 'discogs']);
  assert.deepEqual(moreToRead({ more: { musicbrainz: false, discogs: true } }), ['discogs']);
  assert.deepEqual(moreToRead({ more: { musicbrainz: false, discogs: false } }), []);
});

test("Discogs is credited where a row came from it, not for MusicBrainz's", () => {
  assert.equal(fromDiscogs([row({ release_group: 'g1' })]), false);
  assert.equal(fromDiscogs([row({ release_group: 'g1', discogs_master: 4 })]), false, 'a record MusicBrainz knows too');
  assert.equal(fromDiscogs([row({ release_group: 'g1' }), row({ discogs_release: 99 })]), true);
});

test("the page's picture: its first four records' covers, else the first", () => {
  const c = (n: number) => row({ cover: `/c/${n}` });
  assert.deepEqual(labelCovers([c(1), row(), c(2), c(3), c(4), c(5)]), ['/c/1', '/c/2', '/c/3', '/c/4']);
  assert.deepEqual(labelCovers([c(1), c(2)]), ['/c/1']);
  assert.deepEqual(labelCovers([row()]), []);
});

test('a row opens its record, else is opened by its references and tied to the label', () => {
  assert.deepEqual(howToOpen(rowOpenable(row({ record: 'al-9', release_group: 'g1' })), 'la-5'), { record: 'al-9' });
  assert.deepEqual(howToOpen(rowOpenable(row({ discogs_release: 99, year: 1992 })), 'la-5'), {
    open: { discogs_release: 99, title: 'Selected Ambient Works 85-92', artist: 'Aphex Twin', year: 1992, label: 'la-5' },
  });
});

// F10.1 (the owner's F10, preview 1: "it's now hardly clear which ones I own"): the label page shows what you have apart from what
// you don't. "In your library" at once and never moved by the list's arrival; "Not in your library" the section loader while
// the list is read, then its records, or why in F1's words.
const source = (file: string) => readFileSync(new URL(`../src/lib/${file}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const two = (looking: boolean, extra: Record<string, unknown> = {}) => ({
  id: 'la-5',
  name: 'Warp',
  had: [row({ title: 'Had One', had: true, cover: '/c/h1' }), row({ title: 'Had Two', had: true })],
  not_had: [row({ title: 'Not Had', cover: '/c/n1' })],
  looking,
  more: { musicbrainz: false, discogs: false },
  ...extra,
});

test("the label page's every record, what you have first: its picture's and its count's", () => {
  const p = two(false);
  assert.deepEqual(labelRows(p).map((r) => r.title), ['Had One', 'Had Two', 'Not Had']);
  assert.deepEqual(labelCovers(labelRows(p)), ['/c/h1']);
  const view = source('PlaylistView.svelte');
  assert.match(view, /covers = labelCovers\(labelRows\(p\)\);/);
  // SB4 (changed by design): the header counts what the library has, never the rows read so far.
  assert.match(view, /if \(label\) return labelHead \? labelCount\(labelHead\) : '';/);
  assert.doesNotMatch(view, /labelHead\??\.rows/);
});

test('Not in your library: the section loader naming what is read while the list is read, then its records, or why in F1\'s words', () => {
  assert.deepEqual(sectionPlace({ ...two(true), reading: 'discogs' }, 0, when), { reading: 'Reading Discogs…' });
  assert.equal(sectionPlace(two(false), 1, when), 'rows');
  assert.deepEqual(sectionPlace(two(false, { no_list: [{ source: 'discogs', said: 'unanswered' }] }), 0, when), { line: "No list: Discogs didn't answer" });
  assert.deepEqual(sectionPlace(two(false, { no_list: [{ source: 'musicbrainz', said: 'off' }] }), 0, when), { line: 'No list: MusicBrainz is switched off' });
});

test("LabelBody: In your library at once, apart, before and never in the loader's place; Not in your library its own section", () => {
  const body = source('LabelBody.svelte');
  assert.match(body, /const had = \$derived\(page\.had\.filter\(keep\)\);/);
  assert.match(body, /const notHad = \$derived\(page\.not_had\.filter\(keep\)\);/);
  assert.match(body, /\{#if had\.length\}\s*<section aria-label="In your library">\s*<SectionHead title="In your library" \/>\s*<ul class="rows">\s*\{#each had as r, i \(i\)\}\{@render row\(r, `had:\$\{i\}`\)\}\{\/each\}\s*<\/ul>\s*<\/section>\s*\{\/if\}/);
  assert.match(body, /const notHadPlace = \$derived\(searching \? \(notHad\.length \? 'rows' : null\) : sectionPlace\(page, notHad\.length\)\);/);
  assert.match(
    body,
    /\{#if notHadPlace\}\s*<section aria-label="Not in your library">\s*<SectionHead title="Not in your library" \/>\s*\{#if notHadPlace === 'rows'\}[\s\S]*?\{#each notHad as r, i \(i\)\}\{@render row\(r, `not:\$\{i\}`\)\}\{\/each\}[\s\S]*?\{:else if 'reading' in notHadPlace\}<PageLoading><p class="opening">\{notHadPlace\.reading\}<\/p><\/PageLoading>\{:else\}<p class="empty">\{notHadPlace\.line\}<\/p>\{\/if\}\s*<\/section>\s*\{\/if\}/,
  );
  assert.ok(body.indexOf('<section aria-label="In your library">') < body.indexOf('<section aria-label="Not in your library">'), 'what you have first');
  assert.doesNotMatch(body, /page\.rows|in your library<\/span>/, 'no one list mixing them, no mark the sections say');
  assert.match(body, /setContext\(HOLD, undefined\);/, 'the first section is never held back by the second one\'s read');
});

test('the label page is asked again while its list is read, by lookAgainAfter, with no last ask', () => {
  const body = source('LabelBody.svelte');
  assert.match(body, /lookAgainAfter\(looked\.n\+\+\)/);
  assert.doesNotMatch(body, /LOOK_TRIES|LOOK_MS/);
  assert.match(source('labelPage.ts'), /had: LabelRow\[\]; not_had: LabelRow\[\]; looking: boolean;/);
});

test('SB4.1: the header counts "In your library", in the records\' one word; none had, no count', () => {
  const row = (kind: string | null) => ({ title: 't', artist: 'a', kind, had: true }) as never;
  const notHad = Array.from({ length: 112 }, () => ({ title: 'n', artist: 'a', had: false }) as never);
  assert.equal(labelCount({ had: [row('Album'), row('Album'), row('Album')] }), '3 albums in your library');
  assert.equal(labelCount({ had: [row('EP'), row('EP')] }), '2 EPs in your library');
  assert.equal(labelCount({ had: [row('Album'), row('EP'), row(null)] }), '3 records in your library');
  assert.equal(labelCount({ had: [row('Single')] }), '1 single in your library');
  assert.equal(labelCount({ had: [] }), '', 'none had: no count, never "0"');
  // SB4.2: More reads the next hundred: the header is unchanged; a record that came into the library counts at the next read.
  const page = { had: [row('Album'), row('Album'), row('Album')], not_had: notHad };
  const more = { ...page, not_had: [...notHad, ...notHad] };
  assert.equal(labelCount(more), labelCount(page));
  assert.equal(labelCount({ ...page, had: [...page.had, row('Album')] }), '4 albums in your library');
});
