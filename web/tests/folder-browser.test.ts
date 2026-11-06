import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

// MenuSheet draws its actions only where there is no back row, and every folder but the top has one. The sheet does not
// run here, so this reads it: choosing a folder must not be one of those actions.
const source = readFileSync(new URL('../src/lib/settings/FolderBrowser.svelte', import.meta.url), 'utf8');

test('a folder is chosen from a row, which a folder with a back row still shows', () => {
  assert.doesNotMatch(source, /actions=\{/);
  assert.match(source, /<MenuEntry icon="circlePlus" label=\{choose\}/);
});

// The one loading rule (lib/loader): the list shown stays while a folder opens, the loader takes its place when it is due,
// and with nothing shown yet, nor an error, the loader's place is all there is; "No folders here." only with a listing.
test('a folder on its way keeps what shows, then the loader; none shown yet is the loader, never a blank', () => {
  assert.match(source, /keepWhile\(/);
  assert.match(source, /\{#if wait\.loader \|\| \(!at && !error\)\}\s*<Asking now=\{wait\.loader\} \/>/);
  assert.match(source, /\{#if at\}<p class="hint">No folders here\.<\/p>\{\/if\}/);
});

test("only the last folder asked for answers: an earlier one's coming does not end the wait", () => {
  const open = source.slice(source.indexOf('async function open'), source.indexOf('open(null);'));
  assert.match(open, /if \(mine === asked\) at = listing/);
  assert.match(open, /if \(mine === asked\) waiting = 0/);
});

test("a sheet's actions show on a step with a back row too", () => {
  const sheet = readFileSync(new URL('../src/lib/MenuSheet.svelte', import.meta.url), 'utf8');
  const back = sheet.slice(sheet.indexOf('class="back-row"'), sheet.indexOf(':else if actions.length'));
  assert.match(back, /actions\.filter/);
});
