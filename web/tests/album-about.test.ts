// An album's About (lib/AlbumAbout): where a record comes from (F7: a chip per source, the tracklist's marked) and its labels
// opening their pages where they have a reference (F9). What it draws is read from the source: it takes a whole app to run.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { sourceChips, TRACKLIST } = await import('../src/lib/recordNotHad.ts');
const { ENTRY_WORDS, ICON_OF } = await import('../src/lib/menuTable.ts');
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

test("About's Sources row: labelled as its other rows, a chip per source opening its link sheet (F19.3), the tracklist's marked with Media provider's separator", () => {
  assert.match(
    about,
    /\{#if sourced\.length\}\s*<h3>Sources<\/h3>\s*<p class="chips">\{#each sourced as s \(s\.url\)\}<button class="chip" onclick=\{\(\) => \(linked \?\?= s\)\}>\{s\.name\}\{#if s\.tracks\}<span class="sep">·<\/span>\{TRACKLIST\}\{\/if\}<\/button>\{\/each\}<\/p>\s*\{\/if\}/,
  );
  assert.match(about, /const sourced = \$derived\(sources \? sourceChips\(sources, discogs\?\.url\) : \[\]\);/);
  assert.match(about, /\{provider\.server\}<span class="sep">·<\/span>/, "the separator is Media provider's");
  const view = source('PlaylistView.svelte');
  assert.equal(view.match(/<AlbumAbout source=\{album\.source\} album=\{album\.id\}[^>]*sources=\{albumHead\.sources \?\? \[\]\} onsaved=\{\(\) => saved\+\+\} \/>/g)?.length, 2, 'a record not had and an album had alike');
  assert.match(source('api.ts'), /sources\?: RecordSource\[\];/);
});

// F9.1 (every label in About opens its page where it has a reference).
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

// F19 with SB18: one chip per source, and one word for its mark.
const G = 'https://musicbrainz.org/release-group/00000000-0000-4000-8000-000000000a01';
const R = 'https://musicbrainz.org/release/00000000-0000-4000-8000-000000000a02';
const M = 'https://www.discogs.com/master/4321';
const D = 'https://www.discogs.com/release/8765';
const P = 'https://kitetheory.bandcamp.com/album/answered';

test('one chip per source, opening its album page there, the exact edition kept on the same chip', () => {
  const sources = [
    { source: 'musicbrainz' as const, kind: 'release-group', url: G, tracks: true as const },
    { source: 'musicbrainz' as const, kind: 'release', url: R },
    { source: 'discogs' as const, kind: 'master', url: M },
    { source: 'discogs' as const, kind: 'release', url: D },
  ];
  assert.deepEqual(sourceChips(sources), [
    { name: 'MusicBrainz', url: G, edition: R, tracks: true },
    { name: 'Discogs', url: M, edition: D, tracks: false },
  ]);
  assert.notDeepEqual(sourceChips(sources).map((c) => c.name), ['MusicBrainz', 'MusicBrainz', 'Discogs', 'Discogs']);
});

test('a source known only by its release: the chip opens that release, with no edition', () => {
  assert.deepEqual(sourceChips([{ source: 'musicbrainz', kind: 'release', url: R }]), [{ name: 'MusicBrainz', url: R, tracks: false }]);
  assert.deepEqual(sourceChips([{ source: 'bandcamp', kind: 'album', url: P }]), [{ name: 'Bandcamp', url: P, tracks: false }]);
});

test("a chip's tap opens the link sheet: the album's page there, the edition, and References for an administrator", () => {
  assert.match(about, /<LinkSheet\s+url=\{s\.url\}\s+name=\{s\.name\}\s+also=\{s\.edition \? \[\{ label: ENTRY_WORDS\.openEdition, url: s\.edition \}\] : \[\]\}\s+onreferences=\{me\.admin && source === 'cratarr' \? \(\) => \(\(linked = null\), \(referencesOpen = true\)\) : undefined\}/);
  assert.match(about, /<MenuSheet label="References"[^>]*>\s*<RecordReferences \{album\} onchange=\{\(\) => \(referencesChanged = true\)\} \/>\s*<\/MenuSheet>/, 'the References sheet Health shows, over the page');
  const sheet = source('LinkSheet.svelte');
  assert.match(sheet, /\{#each also as a \(a\.url\)\}<MenuEntry icon=\{ICON_OF\.outside\} label=\{a\.label\}/);
  assert.match(sheet, /\{#if onreferences\}<MenuEntry icon="link" label=\{ENTRY_WORDS\.references\} onclick=\{onreferences\} \/>\{\/if\}/);
  assert.match(sheet, /<MenuEntry icon="externalLink" label="Open" onclick=\{open\} \/>\s*<MenuEntry icon="copy" label="Copy link" onclick=\{copy\} \/>/, 'every other link asks as before');
  assert.equal(ENTRY_WORDS.openEdition, 'Open the edition');
  assert.equal(ENTRY_WORDS.references, 'References…');
  assert.equal(ICON_OF.outside, 'externalLink');
});

test('on a record\'s page Elsewhere is no row of its own: the Discogs lookup is the Discogs chip where the record has none', () => {
  const X = 'https://www.discogs.com/master/9999';
  assert.deepEqual(sourceChips([{ source: 'musicbrainz', kind: 'release-group', url: G, tracks: true }], X), [
    { name: 'MusicBrainz', url: G, tracks: true },
    { name: 'Discogs', url: X, tracks: false },
  ], '(a) the lookup is the Discogs chip, in counting order');
  assert.deepEqual(sourceChips([{ source: 'discogs', kind: 'master', url: M }], X), [{ name: 'Discogs', url: M, tracks: false }], '(b) one Discogs chip, its own');
  assert.deepEqual(sourceChips([{ source: 'bandcamp', kind: 'album', url: P }], X).map((c) => c.name), ['Discogs', 'Bandcamp'], 'counting order');
  assert.match(about, /\{#if !sources && more\?\.links\?\.length\}\s*<h3>Elsewhere<\/h3>/, '(c) a Bandcamp album, no record, keeps its links under Elsewhere');
  assert.match(about, /\{:else if !sources && discogs\?\.url\}\s*<h3>Elsewhere<\/h3>/);
});

test('one word for the source the tracklist comes from: About and References say Tracklist from one constant', () => {
  const refs = source('RecordReferences.svelte');
  assert.equal(TRACKLIST, 'Tracklist');
  assert.match(refs, /import \{ TRACKLIST \} from '\.\/recordNotHad';/);
  assert.match(refs, /value=\{counts \? TRACKLIST : null\}/);
  assert.doesNotMatch(refs, /'Counts'/, 'never Counts');
  assert.match(about, /import \{ sourceChips, TRACKLIST, type SourceChip \} from '\.\/recordNotHad';/);
  assert.doesNotMatch(about, />Tracklist</, 'neither writes the word itself');
});

test("About's mark and References' mark judge the same thing (F19.6, the page half)", () => {
  // (a) a had record whose counting list is MusicBrainz's: About marks MusicBrainz, as References marks its counting source.
  const had = sourceChips([{ source: 'musicbrainz', kind: 'release-group', url: G, tracks: true }, { source: 'discogs', kind: 'master', url: M }]);
  assert.deepEqual(had.filter((c) => c.tracks).map((c) => c.name), ['MusicBrainz']);
  // (b) F13: a record not had whose list came from Discogs while MusicBrainz was out of reach: About marks Discogs (the list it shows).
  const notHad = sourceChips([{ source: 'musicbrainz', kind: 'release-group', url: G }, { source: 'discogs', kind: 'release', url: D, tracks: true }]);
  assert.deepEqual(notHad.filter((c) => c.tracks).map((c) => c.name), ['Discogs']);
});

test('Use this while the page is open: closing References after a change reads the record again; two taps, one sheet', () => {
  assert.match(about, /function closeReferences\(\) \{\s*referencesOpen = false;\s*if \(referencesChanged\) onsaved\?\.\(\);/);
  assert.match(source('RecordReferences.svelte'), /refs = await setReferences\(album, choice\);\s*loadHealth\(\);\s*onchange\?\.\(\);/);
  const view = source('PlaylistView.svelte');
  assert.equal(view.match(/sources=\{albumHead\.sources \?\? \[\]\} onsaved=\{\(\) => saved\+\+\} \/>/g)?.length, 2, "the page's saved bumped");
  assert.match(about, /onclick=\{\(\) => \(linked \?\?= s\)\}/, 'a second tap opens no second sheet');
});
