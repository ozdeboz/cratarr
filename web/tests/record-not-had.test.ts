// A record not had (C54, W9; lib/recordNotHad): its page from the catalogue, nothing of a copy's. Want is in its header, or
// Requested while its request is open; its Bandcamp preview plays from there (OQ14); one had through another record says so
// with no Want; a retired one says until when it was in Music (OQ16).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { albumRowSub, emptyLine, lookAgainAfter, NOT_HAD_ENTRIES, notHadPage, offeredNotHad, wasInMusic } = await import('../src/lib/recordNotHad.ts');
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

test('a record not had whose list the server reads says so', () => {
  assert.equal(emptyLine({ not_had: true, looking: true }, when), 'Reading its tracklist…');
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

test("an empty album's line is emptyLine's, in the same .empty paragraph where \"No tracks yet.\" sat", () => {
  assert.match(view, /import \{ albumRowSub, emptyLine, lookAgainAfter, notHadPage, type NotHadAction \} from '\.\/recordNotHad';/);
  assert.match(view, /\{:else if rows && !rows\.length\}\s*\{#if nothing\}[\s\S]{0,300}?\{:else\}\s*(<!--[^>]*-->\s*)?<p class="empty">\{emptyLine\(notHad && albumHead \? albumHead : \{\}\)\}<\/p>/);
  assert.equal(view.split('<p class="empty">No tracks yet.</p>').length - 1, 1, "only several playlists' sections keep it");
  assert.match(view, /'was_until' \| 'bandcamp' \| 'looking' \| 'no_list'>/, "the album's head keeps them");
  assert.match(view, /was_until: r\.was_until, bandcamp: r\.bandcamp, looking: r\.looking, no_list: r\.no_list \};/);
});

test('a record not had is asked again through lookAgainAfter while the server reads, until it stops or the page shows another', () => {
  assert.match(view, /if \(want === id && r\.not_had\) lookAgain\(want, !!r\.looking\);/);
  assert.match(
    view,
    /function lookAgain\(subject: string, reading: boolean\) \{\s*clearTimeout\(lookTimer\);\s*if \(lookedFor\.subject !== subject \|\| !reading\) lookedFor = \{ subject, n: 0 \};\s*if \(reading\) lookTimer = setTimeout\(\(\) => id === subject && saved\+\+, lookAgainAfter\(lookedFor\.n\+\+\)\);\s*\}/,
  );
  assert.doesNotMatch(view, /LOOK_TRIES|LOOK_MS/, 'no last ask');
});
