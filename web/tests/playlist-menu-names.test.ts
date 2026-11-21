// Rename and Copy name a playlist by its name (MAP-DC1b, Part 1 §5 item 3): since decision 0101 a playlist's id is `pl-…`, and
// Rename opened with that id in its field while Copy's toast said "Copied to pl-…". The sheet takes a whole app, so its source is
// read: what Rename puts in the field when it opens, and what Copy's toast says.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const menu = readFileSync(new URL('../src/lib/PlaylistMenu.svelte', import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test("Rename opens with the playlist's name, not its id", () => {
  const opens = menu.match(/label="Rename"\s+onclick=\{\(\) => \{\s*nameField = ([^;\n]+);/);
  assert.ok(opens, "Rename's entry sets the field as it opens");
  assert.equal(opens[1], 'one.name');
  // The same name again is no rename: its button waits for another.
  assert.ok(menu.includes('disabled={working || !nameField.trim() || nameField.trim() === one.name}'));
  assert.ok(!/nameField = one\.id|nameField\.trim\(\) === one\.id/.test(menu), 'the id is never the name');
});

test("Copy's toast names the playlist copied, not the copy's id", () => {
  const said = menu.match(/withUndo\(made\.length === 1 \? `Copied ([^`]*)\.`/);
  assert.ok(said, "Copy's toast for one playlist");
  assert.equal(said[1], '${names[0]}');
  assert.ok(menu.includes('names.push(p.name);'), 'the name of each one copied');
  assert.ok(menu.includes('onclick={() => copy(theirs)}'), 'Copy is given the playlists, names and all');
  assert.ok(!menu.includes('${made[0]}'), 'never the id the server answers');
});
