// CU18 (on CU17: a library by its id, `LibrarySetting { id, path }`): Libraries' page goes by each library's id, and Cratarr's own
// library gets "Change folder", the existing folder browser choosing its new folder, its id and everything set on it kept. Red first:
// lib/settings/libraryIds did not exist, and the page went by paths. (The whole path, a library moved keeping its switch, is the
// server's e2e `a_library_moved_in_settings_keeps_its_switch_end_to_end`.)
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { folderMoved, libraryOf } = await import('../src/lib/settings/libraryIds.ts');
const source = (f: string) => readFileSync(new URL(`../src/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

const LISTED = [
  { id: 'fo-1', folders: ['/media/music'], paths: ['/media/music'] },
  { id: 'fo-2', folders: ['/media/incoming'], paths: ['/media/incoming'] },
];

test("a folder's library is the server's id for it; a folder just chosen goes by its path until the server has read it", () => {
  assert.equal(libraryOf(LISTED, '/media/music'), 'fo-1');
  assert.equal(libraryOf(LISTED, '/media/incoming'), 'fo-2');
  assert.equal(libraryOf(LISTED, '/media/new'), '/media/new');
  assert.equal(libraryOf([], '/media/music'), '/media/music');
});

test('Change folder: the folder changes in its place, the library the same; never onto a folder chosen already', () => {
  const chosen = ['/media/music', '/media/incoming'];
  assert.deepEqual(folderMoved(chosen, '/media/music', '/mnt/music'), ['/mnt/music', '/media/incoming'], 'its place in the order kept');
  assert.equal(folderMoved(chosen, '/media/music', '/media/incoming'), null, 'a library of its own');
  assert.equal(folderMoved(chosen, '/media/gone', '/mnt/x'), null);
  assert.equal(folderMoved(chosen, '/media/music', '/media/music'), null, 'no move');
  assert.deepEqual(chosen, ['/media/music', '/media/incoming'], 'what was chosen is not changed in place');
});

test("Libraries' page goes by each library's id: its use, name, dot and page", () => {
  const page = source('lib/settings/LibrariesPart.svelte');
  assert.ok(page.includes('const id = libraryOf(d.view?.server_libraries ?? [], path);\n        const u = ed.useOf(d, id);'));
  assert.ok(page.includes("name: ed.policyOf(d, id).name?.trim() || folderName(path),"));
  assert.ok(!page.includes('ed.useOf(d, path)'), 'never a path for a library');
  // Removing a folder sets its library aside by id, and drops the folder by its path.
  assert.match(page, /function removeFolder\(l: Lib\) \{\n    ed\.setUse\(l\.d, l\.id, 'unused'\);\n    l\.d\.values\.folders = chosenOf\(l\.d\)\.filter\(\(p\) => p !== l\.where\);/);
  // The order sent is the list's, by id.
  assert.ok(source('lib/settings/editor.svelte.ts').includes('(d.values.folders as string[]).map((p) => libraryOf(d.view?.server_libraries ?? [], p))'));
});

test('"Change folder" under the folder, by the browser every folder is chosen in, keeps the id and sets its path', () => {
  const page = source('lib/settings/LibrariesPart.svelte');
  assert.ok(page.includes('{#if l.folder}<Item name="Change folder" onclick={() => (moving = l)} />{/if}'));
  assert.ok(page.includes('<FolderBrowser chosen={chosenOf(l.d)} choose="Use this folder" onadd={(p) => moveFolder(l, p)} onclose={() => (moving = null)} />'));
  assert.match(page, /const folders = folderMoved\(chosenOf\(l\.d\), l\.where, to\);\n    if \(!folders\) return;\n    l\.d\.values\.folders = folders;\n    ed\.setPath\(l\.d, l\.id, to\);/);
  const editor = source('lib/settings/editor.svelte.ts');
  assert.match(editor, /setPath = \(d: Draft, id: string, path: string\) => \{\n    const l = d\.libraries\.find\(\(x\) => x\.id === id\);\n    if \(l\) l\.path = path;/);
  assert.ok(source('lib/api.ts').includes('  path?: string;\n  kind: \'main\' | \'incoming\';'));
});
