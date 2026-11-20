// Find more music (the sweep of 1.1, part B): SB12, its words; SB29, new words keep what was shown and a failed next page says so.
// Red first: each test fails on the page as it was at 7da786c91. The page takes a whole app to run, so its source is read; the record
// word rule and the one loading rule it reads are their own modules' tests (record-types.test.ts, loader.test.ts).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const find = readFileSync(new URL('../src/Find.svelte', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const { recordsTitle } = await import('../src/lib/recordTypeRules.ts');

test('SB12: the empty page says what it asks, both its sources', () => {
  assert.ok(find.includes('<p class="note">Search MusicBrainz and Discogs</p>'));
  assert.ok(!find.includes('to find it on MusicBrainz'), 'never MusicBrainz alone: Discogs is asked too');
});

test('SB12: no line saying what the page is', () => {
  assert.ok(!find.includes('Beyond Cratarr for'));
});

test("SB12: the records' heading by the record word rule, never Albums over EPs and singles", () => {
  assert.ok(find.includes('<SectionHead title={recordsTitle(shown.r.albums.map((a) => a.type))} />'));
  assert.ok(!find.includes('<SectionHead title="Albums" />'));
  // What the heading reads: Records until the server says each one's type; their one type where they share it.
  assert.equal(recordsTitle([undefined, undefined]), 'Records');
  assert.equal(recordsTitle(['Album', 'Album']), 'Albums');
  assert.equal(recordsTitle(['Album', 'EP']), 'Records');
});

test('SB29: new words keep what was shown while they are asked, the loader only once due (the one loading rule)', () => {
  assert.match(find, /const kept = keepWhile\(\n    \(\) => found,\n    \(\) => !!q && !error && found\?\.q !== q,\n    \(\) => q,\n  \);/);
  assert.ok(find.includes('const shown = $derived(kept.loader ? null : kept.value);'));
  assert.ok(find.includes('{:else if !shown}\n    <PageLoading />'));
  // Never the loader at once for words not yet answered.
  assert.ok(!find.includes("{:else if !found || found.q !== q}"));
  // "Nothing found" names the words its answer is for.
  assert.ok(find.includes('<p class="note">Nothing found for “{shown.q}”.</p>'));
});

test('SB29: a failed next page says so with Try again, and never ends the list as if it were all', () => {
  assert.ok(!find.includes('found = { ...f, done: true }'), 'a failure is no end');
  assert.ok(find.includes("if (found === f) pageFailed = e instanceof Error ? e.message : 'The search could not be asked';"));
  assert.ok(find.includes('<Notice text={pageFailed} onretry={moreAgain} away="Find more music needs Cratarr." />'));
  // While it is said, nothing asks again by itself; Try again asks once.
  assert.ok(find.includes('{#if !shown.done && !pageFailed}<li class="more" {@attach nearEnd}></li>{/if}'));
  assert.match(find, /const moreAgain = \(\) => \{\n    pageFailed = null;\n    void nextPage\(\);\n  \};/);
  // New words forget it.
  assert.match(find, /error = null;\n    pageFailed = null;/);
});
