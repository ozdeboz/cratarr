// A record not had (C54, W9; lib/recordNotHad): its page from the catalogue, nothing of a copy's. Want is in its header, or
// Requested while its request is open; its Bandcamp preview plays from there (OQ14); one had through another record says so
// with no Want; a retired one says until when it was in Music (OQ16).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { albumRowSub, emptyLine, emptyPlace, keptRows, listCount, lookAgainAfter, NOT_HAD_ENTRIES, notHadPage, offeredNotHad, wasInMusic } = await import('../src/lib/recordNotHad.ts');
const { when } = await import('../src/lib/settings/when.ts');
const { MENUS } = await import('../src/lib/menuTable.ts');

const wanted = (state: string) => ({ state: { state } as never });

test('a record not had has no heart and no Resume; Want is in its header', () => {
  const p = notHadPage({ not_had: true });
  assert.equal(p.heart, false);
  assert.equal(p.resume, false);
  assert.deepEqual(p.actions, ['want']);
  assert.deepEqual(p.lines, []);
});

test('its Bandcamp preview plays from its header where it has one (OQ14), before Want', () => {
  assert.deepEqual(notHadPage({ not_had: true, bandcamp: 'https://a.bandcamp.com/album/b' }).actions, ['play', 'shuffle', 'next', 'last', 'want']);
  assert.deepEqual(notHadPage({ not_had: true, bandcamp: null }).actions, ['want']);
});

test('Requested while its request is open; Want again once it was declined, taken back or rejected', () => {
  for (const s of ['waiting_for_approval', 'wanted', 'partly_there', 'arrived']) assert.deepEqual(notHadPage({ request: wanted(s) }).actions, ['requested'], s);
  for (const s of ['declined', 'cancelled', 'rejected']) assert.deepEqual(notHadPage({ request: wanted(s) }).actions, ['want'], s);
  assert.deepEqual(notHadPage({ request: null }).actions, ['want']);
});

test('had through another record: In your library, opening that one, and no Want', () => {
  const p = notHadPage({ have: 'al-7', bandcamp: 'https://a.bandcamp.com/album/b' });
  assert.deepEqual(p.actions, ['play', 'shuffle', 'next', 'last']);
  assert.deepEqual(p.lines, [{ kind: 'have', record: 'al-7', text: 'In your library' }]);
});

test('a retired record says until when it was in Music (OQ16)', () => {
  const t = Date.UTC(2026, 2, 3, 12) / 1000;
  const line = wasInMusic(t);
  assert.equal(line, `Was in Music until ${new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' })}`);
  assert.match(line, /2026/);
  assert.deepEqual(notHadPage({ was_until: t }).lines, [{ kind: 'was', text: line }]);
});

test("its menu offers only what needs no copy: View, Go to, Share, the searches; each the album menu's own", () => {
  const album = MENUS.album.cards.flatMap((c) => c.entries);
  for (const e of NOT_HAD_ENTRIES) assert.ok(album.includes(e), e);
  assert.deepEqual([...NOT_HAD_ENTRIES].sort(), ['geniusSearch', 'goArtist', 'share', 'view', 'webSearch']);
  for (const e of ['favourite', 'rating', 'addToPlaylist', 'download', 'keep', 'edit', 'changeCover', 'moveTo', 'deleteAlbum'] as const) assert.ok(!offeredNotHad(e), e);
});

test('its rows play nothing and say no missing file; a copy short of a file still says so', () => {
  assert.equal(albumRowSub({ missing: true, artist: 'A' }, true, 'A'), undefined);
  assert.equal(albumRowSub({ missing: true, artist: 'B' }, true, 'A'), 'B');
  assert.equal(albumRowSub({ missing: true, artist: 'A' }, false, 'A'), 'Missing file');
  assert.equal(albumRowSub({ missing: false, artist: 'B' }, false, 'A'), 'B');
});

// F1.12 (the owner's F1, preview 1: "Answered" opened with "No tracks yet"): the page says why a record not had has no
// tracks, from the server's `no_list`, in the same `.empty` paragraph; and keeps asking while the server reads its list.
type Said = 'off' | 'held' | 'unanswered' | 'none';
type Src = 'musicbrainz' | 'discogs' | 'bandcamp';
const of = (source: Src, said: Said, until?: number) => (until === undefined ? { source, said } : { source, said, until });
const notHad = (...no_list: ReturnType<typeof of>[]) => ({ not_had: true, no_list });
const T = Date.UTC(2026, 10, 18, 21, 30) / 1000;
const at = (t: number) => `at ${t}`;

// F6.1 (the owner's F6, preview 1: no loader while a record not had's list was read; F1.12's first bullet as the owner amended
// it): while the server reads it, the track list's place is the standard loader naming the source it reads now, never a line;
// rows replace it; F1's reason once the read ends with none; once rows show, never the loader again for that record.
test("while the server reads a record not had's list, its track list's place is the standard loader naming the source, never a line", () => {
  for (const [reading, name] of [['musicbrainz', 'MusicBrainz'], ['discogs', 'Discogs'], ['bandcamp', 'Bandcamp']] as const)
    assert.deepEqual(emptyPlace({ not_had: true, looking: true, reading }, when), { reading: `Reading its tracklist from ${name}` }, reading);
  for (const v of [{ not_had: true, looking: true, reading: 'musicbrainz' as const, no_list: [of('discogs', 'off')] }, { not_had: true, looking: true, reading: 'bandcamp' as const, no_list: [] }])
    assert.ok(!('line' in emptyPlace(v, when)), `no line while it reads: ${JSON.stringify(v)}`);
});

test("when the read ends with none, F1's reason takes the loader's place", () => {
  const ended = notHad(of('musicbrainz', 'unanswered'), of('bandcamp', 'unanswered'));
  assert.deepEqual(emptyPlace(ended, when), { line: "No tracklist: MusicBrainz and Bandcamp didn't answer" });
  assert.deepEqual(emptyPlace(ended, when), { line: emptyLine(ended, when) }, "the line is emptyLine's");
  assert.deepEqual(emptyPlace(notHad(), when), { line: 'No tracklist: it has no reference to read one from' });
  assert.deepEqual(emptyPlace({}, when), { line: 'No tracks yet.' }, 'an album or a playlist with no rows, as before');
  assert.deepEqual(emptyPlace({ not_had: false, looking: true }, when), { line: 'No tracks yet.' }, 'only a record not had waits for its list');
  assert.doesNotMatch(readFileSync(new URL('../src/lib/recordNotHad.ts', import.meta.url), 'utf8'), /Reading its tracklist…/, 'the text line went');
});

test('rows replace the loader; once they show, an answer with none while it reads keeps them, never the loader again', () => {
  const shown = [{ id: 't1' }, { id: 't2' }];
  const came = [{ id: 't1' }];
  assert.equal(keptRows(came, null, { not_had: true, looking: true }), came, 'its rows, read meanwhile');
  assert.equal(keptRows(came, shown, { not_had: true }), came, "the answer's rows");
  assert.equal(keptRows([], shown, { not_had: true, looking: true }), shown, 'no flicker back to the loader');
  assert.deepEqual(keptRows([], null, { not_had: true, looking: true }), [], 'none shown yet: the loader');
  assert.deepEqual(keptRows([], [], { not_had: true, looking: true }), []);
  assert.deepEqual(keptRows([], shown, { not_had: true }), [], 'the read ended: what the server has');
  assert.deepEqual(keptRows([], shown, { looking: true }), [], 'only a record not had');
});

test('every source it has a reference of switched off: said in one part', () => {
  assert.equal(emptyLine(notHad(of('musicbrainz', 'off'), of('discogs', 'off')), when), 'No tracklist: MusicBrainz and Discogs are switched off');
  assert.equal(emptyLine(notHad(of('musicbrainz', 'off')), when), 'No tracklist: MusicBrainz is switched off');
});

test('one switched off and one with none: both said', () => {
  assert.equal(emptyLine(notHad(of('musicbrainz', 'off'), of('bandcamp', 'none')), when), 'No tracklist: MusicBrainz is switched off; Bandcamp has none');
});

test('none of its references has a list: each named', () => {
  assert.equal(emptyLine(notHad(of('musicbrainz', 'none'), of('discogs', 'none'), of('bandcamp', 'none')), when), 'No tracklist: MusicBrainz, Discogs and Bandcamp have none');
});

test('Bandcamp held back says until when, as the page says a time, before what has none', () => {
  const line = emptyLine(notHad(of('musicbrainz', 'none'), of('bandcamp', 'held', T)), when);
  assert.equal(line, 'No tracklist: Bandcamp is held back until ' + when(T) + '; MusicBrainz has none');
  assert.equal(emptyLine(notHad(of('musicbrainz', 'none'), of('bandcamp', 'held', T)), at), `No tracklist: Bandcamp is held back until at ${T}; MusicBrainz has none`);
  assert.doesNotMatch(line, /didn't answer/, 'a hold Cratarr keeps itself is no silence');
});

test("a source that didn't answer is said so", () => {
  assert.equal(emptyLine(notHad(of('discogs', 'unanswered')), when), "No tracklist: Discogs didn't answer");
});

test('no reference to read one from', () => {
  assert.equal(emptyLine(notHad(), when), 'No tracklist: it has no reference to read one from');
});

test('parts in the order off, held, unanswered, none; sources in counting order inside a part', () => {
  assert.equal(
    emptyLine(notHad(of('bandcamp', 'none'), of('musicbrainz', 'unanswered'), of('discogs', 'off')), at),
    "No tracklist: Discogs is switched off; MusicBrainz didn't answer; Bandcamp has none",
  );
  assert.equal(
    emptyLine(notHad(of('musicbrainz', 'unanswered'), of('bandcamp', 'held', T), of('discogs', 'off')), at),
    `No tracklist: Discogs is switched off; Bandcamp is held back until at ${T}; MusicBrainz didn't answer`,
  );
  assert.equal(emptyLine(notHad(of('bandcamp', 'none'), of('musicbrainz', 'none')), at), 'No tracklist: MusicBrainz and Bandcamp have none');
  assert.equal(emptyLine(notHad(of('discogs', 'off'), of('musicbrainz', 'off')), at), 'No tracklist: MusicBrainz and Discogs are switched off');
  assert.equal(emptyLine(notHad(of('bandcamp', 'none'), of('discogs', 'none'), of('musicbrainz', 'none')), at), 'No tracklist: MusicBrainz, Discogs and Bandcamp have none');
});

test('an album or a playlist with no rows that is no record not had says "No tracks yet." as before; a record not had never', () => {
  assert.equal(emptyLine({}, when), 'No tracks yet.');
  assert.equal(emptyLine({ not_had: false }, when), 'No tracks yet.');
  assert.equal(emptyLine({ not_had: false, no_list: [] }, when), 'No tracks yet.');
  for (const v of [{ not_had: true }, { not_had: true, looking: true }, notHad(), notHad(of('discogs', 'off'))]) assert.notEqual(emptyLine(v, when), 'No tracks yet.', JSON.stringify(v));
});

test('asked again every 4 s for its first eight asks, then every 30 s, with no last ask', () => {
  for (const n of [0, 1, 7]) assert.equal(lookAgainAfter(n), 4000, String(n));
  for (const n of [8, 9, 60, 1000]) assert.equal(lookAgainAfter(n), 30000, String(n));
});

// The panel's use of them, read from the source, as the view does not run here.
const view = readFileSync(new URL('../src/lib/PlaylistView.svelte', import.meta.url), 'utf8');

test("an empty album's place: PageLoading naming the read while a record not had's list is read, else emptyLine's line in the .empty paragraph where \"No tracks yet.\" sat", () => {
  assert.match(view, /import \{ albumRowSub, emptyPlace, keptRows, listCount, lookAgainAfter, notHadPage, type NotHadAction \} from '\.\/recordNotHad';/);
  assert.match(
    view,
    /\{:else if rows && !rows\.length\}\s*\{#if nothing\}[\s\S]{0,300}?\{:else\}\s*(<!--[^>]*-->\s*)?\{@const place = emptyPlace\(notHad && albumHead \? albumHead : \{\}\)\}\s*\{#if 'reading' in place\}<PageLoading held=\{false\}><p class="opening">\{place\.reading\}<\/p><\/PageLoading>\{:else\}<p class="empty">\{place\.line\}<\/p>\{\/if\}/,
  );
  assert.equal(view.split('<p class="empty">No tracks yet.</p>').length - 1, 1, "only several playlists' sections keep it");
  assert.match(view, /'was_until' \| 'bandcamp' \| 'looking' \| 'no_list' \| 'reading' \| 'sources'/, "the album's head keeps them");
  assert.match(view, /was_until: r\.was_until, bandcamp: r\.bandcamp, looking: r\.looking, no_list: r\.no_list, reading: r\.reading, sources: r\.sources\b/);
  assert.match(view, /\.opening \{\s*margin: 0;\s*color: var\(--muted\);\s*\}/, "said under the mark as PageLoading's own line");
});

test("a record not had's rows, once shown, stay while the server reads again: never the loader again for it", () => {
  assert.match(view, /const kept = keptRows\(r\.rows, rowsFor === want \? rows : null, r\);\s*return \{ \.\.\.r, rows: kept, total: kept\.length \};/);
});

test('a record not had is asked again through lookAgainAfter while the server reads, until it stops or the page shows another', () => {
  assert.match(view, /if \(want === id && r\.not_had\) lookAgain\(want, !!r\.looking\);/);
  assert.match(
    view,
    /function lookAgain\(subject: string, reading: boolean\) \{\s*clearTimeout\(lookTimer\);\s*if \(lookedFor\.subject !== subject \|\| !reading\) lookedFor = \{ subject, n: 0 \};\s*if \(reading\) lookTimer = setTimeout\(\(\) => id === subject && saved\+\+, lookAgainAfter\(lookedFor\.n\+\+\)\);\s*\}/,
  );
  assert.doesNotMatch(view, /LOOK_TRIES|LOOK_MS/, 'no last ask');
});

// PB1 (the review of preview 1's fixes): the track list's loader is a part of the page, not the page: it holds no panel (lib/Hold),
// so a record not had shows its header, Want and About at once while its list is read, and only its track list waits.
const pageLoading = readFileSync(new URL('../src/lib/PageLoading.svelte', import.meta.url), 'utf8');

test("a record not had's track list loader holds no panel: its header, Want and About show at once, only its track list waits", () => {
  assert.match(pageLoading, /let \{ children, held = true \}: \{ children\?: import\('svelte'\)\.Snippet; held\?: boolean \} = \$props\(\);/);
  assert.match(pageLoading, /if \(untrack\(\(\) => held\)\)\s*holdUntil\(/, 'it claims its place only where it loads a page or a panel whole');
  assert.equal(pageLoading.match(/holdUntil\(/g)?.length, 1, 'its one claim');
  assert.equal(view.match(/<PageLoading held=\{false\}>/g)?.length, 1, "the track list's loader alone: the page's own still hold it");
  assert.match(view, /\{#if 'reading' in place\}<PageLoading held=\{false\}><p class="opening">\{place\.reading\}<\/p><\/PageLoading>/);
  assert.match(view, /<p class="empty">\{place\.line\}<\/p>\{\/if\}\s*\{#if notHad\}\{@render albumFoot\(\)\}\{\/if\}/, 'its About under the waiting track list');
  const foot = view.match(/\{#snippet albumFoot\(\)\}([\s\S]*?)\{\/snippet\}/)?.[1] ?? '';
  assert.match(foot, /\{#if notHad\}\s*<AlbumAbout source=\{album\.source\} album=\{album\.id\}[^>]*more=\{albumHead\} labels=\{albumHead\.labels \?\? \[\]\} sources=\{albumHead\.sources \?\? \[\]\} onsaved=\{\(\) => saved\+\+\} \/>/, "a record not had's About");
  assert.match(view, /<SimilarLists kind="crate" id=\{crate\} \/>\{\/if\}\s*\{\/if\}\s*\{@render albumFoot\(\)\}/, 'the same foot under its tracks once they come');
});

// F17: a record not had's list head counts the tracks it lists, never "0 tracks" beside a list or a loader.
test('a record not had counts the tracks it lists, or none; never 0 beside a list', () => {
  assert.equal(listCount({ notHad: true, shown: 11, playable: 0 }), '11 tracks', 'Night Ferry: 11 rows read from MusicBrainz');
  assert.equal(listCount({ notHad: true, shown: 0, playable: 0 }), '', 'its list being read: no count beside the loader');
  assert.equal(listCount({ notHad: true, shown: 3, playable: 0 }), '3 tracks', "the page's own search narrowing it: the rows shown");
  assert.equal(listCount({ notHad: true, shown: 1, playable: 0 }), '1 track');
  for (const shown of [0, 1, 11]) assert.notEqual(listCount({ notHad: true, shown, playable: 0 }), '0 tracks');
  const view = readFileSync(new URL('../src/lib/PlaylistView.svelte', import.meta.url), 'utf8');
  assert.match(view, /count=\{crate \? plural\(total, 'track'\) : listCount\(\{ notHad, shown: shownRows\.length, playable: shownPlayable\.length \}\)\}/, "the list head's count is listCount's");
  assert.doesNotMatch(view, /count=\{plural\(crate \? total : shownPlayable\.length, 'track'\)\}/);
});

test('the count follows the rows kept', () => {
  const eleven = Array.from({ length: 11 }, (_, i) => ({ id: null, title: `Track ${i + 1}` }));
  const reading = { not_had: true, looking: true };
  assert.equal(listCount({ notHad: true, shown: keptRows([], null, reading).length, playable: 0 }), '', 'no rows yet: no count');
  const shown = keptRows(eleven, null, reading);
  assert.equal(listCount({ notHad: true, shown: shown.length, playable: 0 }), '11 tracks', 'its rows come: the count with them');
  assert.equal(listCount({ notHad: true, shown: keptRows([], shown, reading).length, playable: 0 }), '11 tracks', 'an answer with none while it reads again keeps the rows and the count');
});

test("a had album's and a crate's count are as before (F17.3)", () => {
  assert.equal(listCount({ notHad: false, shown: 10, playable: 8 }), '8 tracks', 'a had album: its playable rows (2 missing files left out, SB11 being preview 2\'s)');
  const view = readFileSync(new URL('../src/lib/PlaylistView.svelte', import.meta.url), 'utf8');
  // A pin (green before F17 and after): a crate's count is its total, read or not.
  assert.match(view, /count=\{(plural\(crate \? total : |crate \? plural\(total, 'track'\))/, 'a crate: its total, 340 tracks with 100 read');
});
