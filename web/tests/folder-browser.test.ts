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

test("a sheet's actions show on a step with a back row too", () => {
  const sheet = readFileSync(new URL('../src/lib/MenuSheet.svelte', import.meta.url), 'utf8');
  const back = sheet.slice(sheet.indexOf('class="back-row"'), sheet.indexOf(':else if actions.length'));
  assert.match(back, /actions\.filter/);
});
