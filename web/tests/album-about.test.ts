// An album's About (lib/AlbumAbout): where a record comes from (F7: a chip per source, the tracklist's marked) and its labels
// opening their pages where they have a reference (F9). What it draws is read from the source: it takes a whole app to run.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { sourceChips } = await import('../src/lib/recordNotHad.ts');
const { aboutLabels, labelChip, labelOpenBody, labelOpens } = await import('../src/lib/labelPage.ts');

const source = (file: string) => readFileSync(new URL(`../src/lib/${file}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const about = source('AlbumAbout.svelte');

const GROUP = 'https://musicbrainz.org/release-group/00000000-0000-4000-8000-000000000701';
const PAGE = 'https://kitetheory.bandcamp.com/album/answered';
const RELEASE = 'https://musicbrainz.org/release/00000000-0000-4000-8000-000000000702';

// F7.1 (the owner's F7, preview 1: "I don't know how this page got to be").
test('a record not had from a MusicBrainz release group and a Bandcamp page: a chip each, in order, MusicBrainz marked as giving its tracklist', () => {
  const sources = [
    { source: 'musicbrainz', kind: 'release-group', url: GROUP, tracks: true as const },
    { source: 'bandcamp', kind: 'album', url: PAGE },
  ];
  assert.deepEqual(sourceChips(sources), [
    { name: 'MusicBrainz', url: GROUP, tracks: true },
    { name: 'Bandcamp', url: PAGE, tracks: false },
  ]);
});

test('a record had with a MusicBrainz release: one chip opening that release, none marked', () => {
  assert.deepEqual(sourceChips([{ source: 'musicbrainz', kind: 'release', url: RELEASE }]), [{ name: 'MusicBrainz', url: RELEASE, tracks: false }]);
});

test('a record with no reference has no Sources row', () => {
  assert.deepEqual(sourceChips(undefined), []);
  assert.deepEqual(sourceChips([]), []);
  assert.match(about, /\{#if sourced\.length\}\s*<h3>Sources<\/h3>/, 'the row only where there is a chip');
});

test('each source by its name, in the order the server gives; an address that is no https page is never a chip', () => {
  const master = 'https://www.discogs.com/master/4321';
  assert.deepEqual(
    sourceChips([
      { source: 'musicbrainz', kind: 'release-group', url: GROUP },
      { source: 'discogs', kind: 'master', url: master, tracks: true },
      { source: 'bandcamp', kind: 'album', url: PAGE },
    ]).map((c) => [c.name, c.tracks]),
    [['MusicBrainz', false], ['Discogs', true], ['Bandcamp', false]],
  );
  assert.deepEqual(sourceChips([{ source: 'bandcamp', kind: 'album', url: 'javascript:alert(1)' }, { source: 'discogs', kind: 'release', url: 'http://www.discogs.com/release/1' }]), []);
});

test("About's Sources row: labelled as its other rows, a chip per source opening its page as About's links do, the tracklist's marked with Media provider's separator", () => {
  assert.match(
    about,
    /\{#if sourced\.length\}\s*<h3>Sources<\/h3>\s*<p class="chips">\{#each sourced as s \(s\.url\)\}<a class="chip" href=\{s\.url\} target="_blank" rel="noopener noreferrer">\{s\.name\}\{#if s\.tracks\}<span class="sep">·<\/span>Tracklist\{\/if\}<\/a>\{\/each\}<\/p>\s*\{\/if\}/,
  );
  assert.match(about, /const sourced = \$derived\(sourceChips\(sources\)\);/);
  assert.match(about, /\{provider\.server\}<span class="sep">·<\/span>/, "the separator is Media provider's");
  const view = source('PlaylistView.svelte');
  assert.equal(view.match(/<AlbumAbout source=\{album\.source\} album=\{album\.id\}[^>]*sources=\{albumHead\.sources\}[^>]*\/>/g)?.length, 2, 'a record not had and an album had alike');
  assert.match(source('api.ts'), /sources\?: RecordSource\[\];/);
});

// F9.1 (the coordinator's: every label in About opens its page where it has a reference).
const warp = { id: 'la-5', name: 'Warp', catno: 'WARP123' };

test('F9 (a): labels by their ids: a chip each opening its page, as today', () => {
  assert.deepEqual(aboutLabels([warp], 'Warp', [{ name: 'Warp', catno: 'WARP123', ref: { source: 'discogs', id: '23528' } }]), [warp]);
  assert.equal(labelOpens(warp), 'page');
  assert.equal(labelChip(warp), 'Warp · WARP123');
});

test("F9 (b): only from Discogs' release read: a chip per label, its catalogue number beside it, opening the label found or made by its Discogs id", () => {
  const d = [
    { name: 'Warp', catno: 'WARP123', ref: { source: 'discogs' as const, id: '23528' } },
    { name: 'Bleep', catno: 'none', ref: { source: 'discogs' as const, id: '91' } },
  ];
  assert.deepEqual(aboutLabels([], undefined, d), d);
  assert.deepEqual(aboutLabels(undefined, null, d).map(labelOpens), ['ref', 'ref']);
  assert.deepEqual(d.map(labelChip), ['Warp · WARP123', 'Bleep']);
  assert.deepEqual(labelOpenBody(d[0]), { source: 'discogs', id: '23528', name: 'Warp' });
});

test("F9 (c), (d): the album's label text: a chip opening the one known label of that name; plain text where two share it or none has it", () => {
  assert.equal(labelOpens(aboutLabels([{ id: 'la-7', name: 'Mute' }], 'Mute', undefined)[0]), 'page', '(c) the one label of that name');
  assert.equal(labelOpens(aboutLabels([{ name: 'Mute' }], 'Mute', undefined)[0]), null, '(d) a name two labels share: plain text');
  assert.deepEqual(aboutLabels([], 'Mute', undefined), [{ name: 'Mute' }], 'the text alone');
  assert.equal(labelOpens({ name: 'Mute' }), null);
});

test("F9 (e): a MusicBrainz release's label with its MBID and no row yet: a chip opening the label made by its MBID", () => {
  const mbid = '00000000-0000-4000-8000-000000000801';
  const l = { name: 'Ghostly International', ref: { source: 'musicbrainz' as const, id: mbid } };
  assert.equal(labelOpens(l), 'ref');
  assert.deepEqual(labelOpenBody(l), { source: 'musicbrainz', id: mbid, name: 'Ghostly International' });
  assert.equal(labelOpens({ ...l, id: 'la-9' }), 'page', 'a known row before its reference');
});

test('F9: About draws every label from one list: a chip opening its page, a chip opening it by its reference first, else plain text', () => {
  assert.match(about, /const shownLabels = \$derived\(aboutLabels\(labels, more\?\.label, discogs\?\.labels\)\);/);
  assert.match(about, /\{#if chipped\.length\}<p class="chips">\{#each chipped as l, i \(i\)\}<button class="chip" onclick=\{\(\) => openLabel\(l\)\}>\{labelChip\(l\)\}<\/button>\{\/each\}<\/p>\{\/if\}/);
  assert.match(about, /\{#if plain\.length\}<p class="words">\{plain\.map\(labelChip\)\.join\('\\n'\)\}<\/p>\{\/if\}/);
  assert.match(about, /if \(l\.id\) return viewOver\(labelSubject\(l\.id\)\);/);
  assert.match(about, /const \{ id \} = await openLabelByRef\(labelOpenBody\(l\)\);\s*viewOver\(labelSubject\(id\)\);/);
  assert.match(source('api.ts'), /export const openLabelByRef = \(b: import\('\.\/labelPage'\)\.LabelOpenBody\) => post<\{ id: string \}>\('\/api\/labels\/open', b\);/);
  assert.doesNotMatch(about, /discogs\.labels\.map\(/, "Discogs' labels go through the one list");
});

// PB2 (the review of preview 1's fixes): a Bandcamp album's About is drawn with the labels its answer names, so its label opens the
// one label of its name (F9 (c)); fed the server's answer as it comes (`GET /api/albums/bandcamp/<id>`: the album view, its
// AlbumMore flattened, its `labels` from `labels::of_text`).
const bandcampAnswer = JSON.parse(`{
  "source": "bandcamp", "id": "bc.MHxodHRwczovL2tpdGV0aGVvcnkuYmFuZGNhbXAuY29tL2FsYnVtL2Fuc3dlcmVk", "title": "Answered", "artist": "Kite Theory",
  "year": 2024, "rows": [], "total": 0, "released": "2024-03-01", "label": "Low Tide Records",
  "labels": [{ "name": "Low Tide Records", "id": "la-12" }],
  "links": [{ "name": "Bandcamp", "url": "https://kitetheory.bandcamp.com/album/answered" }]
}`);

test("F9 (c) on a Bandcamp album: the label its answer names, the one label of its name, opens that label's page", () => {
  const view = source('PlaylistView.svelte');
  assert.match(view, /label: r\.label, about: r\.about,[^\n]*, labels: r\.labels, /, 'the page keeps what the answer names');
  const drawn = view.match(/<AlbumAbout\s+source=\{album\.source\}\s+album=\{album\.id\}[^>]*provider=\{\{ server: 'Bandcamp', libraries: \[\] \}\}[^>]*\/>/)?.[0] ?? '';
  assert.match(drawn, /more=\{albumHead\}\s+labels=\{albumHead\.labels \?\? \[\]\}/, "its About drawn with its answer's labels");
  const shown = aboutLabels(bandcampAnswer.labels ?? [], bandcampAnswer.label, undefined);
  assert.deepEqual(shown, [{ name: 'Low Tide Records', id: 'la-12' }]);
  assert.deepEqual(shown.map(labelOpens), ['page']);
  // Two labels of its name: the answer names it with no id, plain text.
  const shared = { ...bandcampAnswer, labels: [{ name: 'Low Tide Records' }] };
  assert.deepEqual(aboutLabels(shared.labels, shared.label, undefined).map(labelOpens), [null]);
});
