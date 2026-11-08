import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { test } from 'node:test';

// The one loading rule in a sheet (lib/loader). The sheets do not run here, so this reads them: a loader in one comes
// through lib/Asking, which times it and sizes it where it is, not a MarkLoader of its own, shown at once at a size typed in.
const lib = new URL('../src/lib/', import.meta.url);
const read = (path: string) => readFileSync(new URL(path, lib), 'utf8');
const svelte = (readdirSync(lib, { recursive: true }) as string[]).map((f) => f.replaceAll('\\', '/')).filter((f) => f.endsWith('.svelte'));

test('no sheet has a MarkLoader of its own but the sign beside the filter\'s count and a list\'s "more is coming"', () => {
  const own = svelte.filter((f) => read(f).includes('<MenuSheet') && read(f).includes('<MarkLoader')).sort();
  assert.deepEqual(own, ['FilterSheet.svelte', 'PlaylistView.svelte']);
  assert.match(read('FilterSheet.svelte'), /\{#if countWait\.loader\}<MarkLoader size=\{18\} \/>/);
});

test("a sheet's steps have none either", () => {
  for (const step of ['TempoStep.svelte', 'PlaylistAbout.svelte', 'PlaylistsStep.svelte', 'WordsStep.svelte']) assert.doesNotMatch(read(step), /<MarkLoader/);
});

test('Asking is never larger than a loader where it is', () => {
  assert.match(read('Asking.svelte'), /most = loaderSizeIn\(el\)/);
  assert.match(read('Asking.svelte'), /<MarkLoader size=\{Math\.min\(size, most\)\} \/>/);
});

// What a list's place says before its answer: the wait, never its emptiness.
test("a track's menu opened on its playlists waits for them before it counts them", () => {
  assert.match(read('TrackMenu.svelte'), /\{#if listsWait\}\s*<Asking \/>\s*\{:else if onServer && track\}/);
});

test('a wait that failed ends: the loader is not there for ever', () => {
  assert.match(read('NotForSheet.svelte'), /\{:else if !unread\}\s*<Asking \/>/);
  assert.match(read('RecordEdit.svelte'), /wordsWait = \$derived\(albums\.length === 1 && !wordsRead && !wordsFailed\)/);
  assert.match(read('TrackMenu.svelte'), /listsWait = \$derived\(.*!other && !otherFailed\)/);
});
