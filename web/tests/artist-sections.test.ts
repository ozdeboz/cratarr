// An artist page's sections that wait on a background read (F8.1): "Not in your library" (/api/artists/missing) and the
// recommendations (/api/artists/related) show the standard loader naming what is read, fill by themselves as it ends, and say a
// failure in F1's words; never a section silently absent. What the page draws is read from the source.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { emptyLine, sectionPlace } = await import('../src/lib/recordNotHad.ts');
const { when } = await import('../src/lib/settings/when.ts');

const source = (file: string) => readFileSync(new URL(`../src/lib/${file}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const body = source('ArtistBody.svelte');
const related = source('RelatedArtists.svelte');
const T = Date.UTC(2026, 10, 18, 21, 30) / 1000;
const at = (t: number) => `at ${t}`;

test("while an artist's section is read: the loader, naming the source it reads now", () => {
  assert.deepEqual(sectionPlace({ looking: true, reading: 'musicbrainz' }, 0, when), { reading: 'Reading MusicBrainz…' });
  assert.deepEqual(sectionPlace({ looking: true, reading: 'bandcamp' }, 0, when), { reading: 'Reading Bandcamp…' });
  assert.deepEqual(sectionPlace({ looking: true, reading: 'listenbrainz' }, 0, when), { reading: 'Reading ListenBrainz…' });
});

test('(a) its records once read fill it; rows already there stay while it reads on', () => {
  assert.equal(sectionPlace({ looking: false }, 3, when), 'rows');
  assert.equal(sectionPlace({}, 1, when), 'rows');
  assert.equal(sectionPlace({ looking: true, reading: 'musicbrainz' }, 2, when), 'rows');
});

test("(b) MusicBrainz and Bandcamp didn't answer: the section says so in F1's words", () => {
  assert.deepEqual(
    sectionPlace({ looking: false, no_list: [{ source: 'musicbrainz', said: 'unanswered' }, { source: 'bandcamp', said: 'unanswered' }] }, 0, when),
    { line: "No list: MusicBrainz and Bandcamp didn't answer" },
  );
});

test("(c) the recommendations with MusicBrainz switched off say so in F1's off words; what ListenBrainz gave still shows", () => {
  assert.deepEqual(sectionPlace({ no_list: [{ source: 'musicbrainz', said: 'off' }] }, 0, when), { line: 'No list: MusicBrainz is switched off' });
  assert.equal(sectionPlace({ no_list: [{ source: 'musicbrainz', said: 'off' }] }, 4, when), 'rows');
  assert.deepEqual(sectionPlace({ no_list: [{ source: 'lastfm', said: 'off' }] }, 0, when), { line: 'No list: Last.fm is switched off' });
});

test("a section's why is F1's words, one way: off, held, unanswered; each part's sources in the order they were read", () => {
  assert.deepEqual(
    sectionPlace({ no_list: [{ source: 'listenbrainz', said: 'unanswered' }, { source: 'bandcamp', said: 'held', until: T }, { source: 'musicbrainz', said: 'off' }] }, 0, at),
    { line: `No list: MusicBrainz is switched off; Bandcamp is held back until at ${T}; ListenBrainz didn't answer` },
  );
  assert.deepEqual(sectionPlace({ no_list: [{ source: 'bandcamp', said: 'unanswered' }, { source: 'musicbrainz', said: 'unanswered' }] }, 0, at), { line: "No list: Bandcamp and MusicBrainz didn't answer" });
  const list = [{ source: 'musicbrainz' as const, said: 'off' as const }, { source: 'bandcamp' as const, said: 'held' as const, until: T }];
  const place = sectionPlace({ no_list: list }, 0, at);
  const tail = (s: string) => s.slice(s.indexOf(': ') + 2);
  assert.ok(place && typeof place === 'object' && 'line' in place, JSON.stringify(place));
  assert.equal(tail(place.line), tail(emptyLine({ not_had: true, no_list: list }, at)), "a record not had's words");
});

test('a read that ended with nothing to show and nothing failed leaves no section; one that failed always says so', () => {
  assert.equal(sectionPlace({ looking: false }, 0, when), null);
  assert.equal(sectionPlace({ no_list: [] }, 0, when), null);
  assert.notEqual(sectionPlace({ no_list: [{ source: 'musicbrainz', said: 'unanswered' }] }, 0, when), null);
});

test('Not in your library: the loader naming the read, its rows, or why; asked again by lookAgainAfter until it is read', () => {
  assert.match(body, /const missingPlace = \$derived\(searching \? \(missingShown\.length \? 'rows' : null\) : missing && sectionPlace\(missing, missingShown\.length\)\);/);
  assert.match(
    body,
    /\{#if missingPlace\}\s*<section aria-label="Not in your library">\s*<SectionHead title="Not in your library" \/>\s*\{#if missingPlace === 'rows'\}[\s\S]*?\{:else if 'reading' in missingPlace\}<PageLoading><p class="opening">\{missingPlace\.reading\}<\/p><\/PageLoading>\{:else\}<p class="empty">\{missingPlace\.line\}<\/p>\{\/if\}\s*<\/section>\s*\{\/if\}/,
  );
  assert.match(body, /if \(r\.looking\) t = window\.setTimeout\(\(\) => ask\(n \+ 1\), lookAgainAfter\(n\)\);/);
  assert.match(body, /void ask\(0\);/);
  assert.doesNotMatch(body, /ask\(5\)|left > 0/, 'no last ask');
  assert.match(source('api.ts'), /quietly<\{ albums: MissingAlbum\[\]; looking: boolean \} & ListRead>\(`\/api\/artists\/missing/);
});

test('the recommendations: the loader naming the read, their artists, or why; asked again by lookAgainAfter until they are read', () => {
  assert.match(related, /const place = \$derived\(answer && sectionPlace\(answer, answer\.artists\.length\)\);/);
  assert.match(
    related,
    /\{#if place\}\s*<section aria-label="Related artists to explore">\s*<SectionHead title="Related artists to explore" \/>\s*\{#if place === 'rows'\}[\s\S]*?\{:else if 'reading' in place\}<PageLoading><p class="opening">\{place\.reading\}<\/p><\/PageLoading>\{:else\}<p class="empty">\{place\.line\}<\/p>\{\/if\}\s*<\/section>\s*\{\/if\}/,
  );
  assert.match(related, /if \(r\.looking\) timer = setTimeout\(look, lookAgainAfter\(n\+\+\)\);/);
  assert.doesNotMatch(related, /TRIES|AGAIN_MS/, 'no last ask');
  assert.match(source('api.ts'), /quietly<\{ artists: RelatedArtist\[\]; looking: boolean \} & ListRead>\(`\/api\/artists\/related/);
});

test("an artist page's sections never hold it: the page shows at once, what is read follows in its place", () => {
  assert.match(body, /setContext\(HOLD, undefined\);/);
  assert.match(body, /import \{ HOLD \} from '\.\/holdPlace\.svelte';/);
});
