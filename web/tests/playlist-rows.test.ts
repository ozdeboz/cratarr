// A playlist viewed (lib/PlaylistView): a list is known by its id, what shows says its name (decision 0101: an id is
// `pl-` and 12 hex digits, no name). Read from the source, as the view does not run here.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const view = readFileSync(new URL('../src/lib/PlaylistView.svelte', import.meta.url), 'utf8');
/** Each call of the snippet that draws a row of a list: what it is given. */
const rows = [...view.matchAll(/@render trackRow\((.*?)\)\}/g)].map((m) => m[1].split(',').map((x) => x.trim()));
const snippet = view.match(/\{#snippet trackRow\((.*)\)\}/)?.[1] ?? '';

test("a row asks what it can be taken out of by its playlist's id, never by the name it shows", () => {
  assert.doesNotMatch(view, /outOf\(\w+, (title|name)\)/, 'a name never matches an id: no swipe left, no Remove from');
  assert.match(snippet, /\blistId\b/, 'a row is given both');
  assert.match(view, /outOf\(r, listId\)/);
  assert.match(view, /rs\.every\(\(r\) => outOf\(r, id\)\)/, 'the select bar asks by the id too');
});

test('a row of several playlists at once is given the name to show and the id to act on', () => {
  const multi = rows.find((a) => a.includes('sid'));
  assert.ok(multi, 'a row of a section');
  assert.equal(multi[2], 'playlistName(sid)', 'its queue and its labels say the name');
});

test('what is said of a playlist says its name, never its id', () => {
  assert.doesNotMatch(view, /`Removed from \$\{list\}\.`/, 'the Undo');
  assert.doesNotMatch(view, /title=\{sid\}/, "a section's head");
  assert.doesNotMatch(view, /aria-label=\{`(Delete|Edit) \$\{id\}`\}/, 'the Delete and Edit buttons');
  assert.match(view, /`Removed from \$\{playlistName\(list\)\}\.`/);
});

test('taking ticked rows out goes by the id, its words by the name', () => {
  assert.match(view, /void outOfPlaylist\(id, rs\)/, 'the id is what the file and the section are found by');
  assert.doesNotMatch(view, /outOfPlaylist\(list, rs\)\) \}\]/);
});
