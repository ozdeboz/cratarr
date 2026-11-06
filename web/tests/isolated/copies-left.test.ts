// What the Library's list does at once when a copy goes or moves, before it is read again (lib/albums): a row that left and
// came back with the next read was a page taking "it left" for "it is gone".
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { copiesGone, copiesMoved, library, libraryDots, listedAlbum } = await import('../../src/lib/albums.svelte.ts');

const lib = (id: string, kind: string) => ({ id, name: id, kind, colour: `#${id}` });
const two = { id: 'al-1', title: 'Two', copies: [{ library: 'music', plays: true }, { library: 'mine' }] };
const one = { id: 'al-2', title: 'One', copies: [{ library: 'music', plays: true }] };
const view = () => ({ sources: [{ id: 'own', albums: [two, one], library_info: [lib('music', 'main'), lib('mine', 'own'), lib('more', 'main')] }], trash_days: 7, music: null }) as never;
const titles = () => library.view!.sources[0].albums.map((a: { title: string }) => a.title);
const librariesHeld = (id: string) => listedAlbum('own', id)?.a.copies?.map((c: { library: string }) => c.library);

test('a record keeps its row when one of its copies goes, and only a last copy takes it away', () => {
  library.view = view();
  const whole = copiesGone([{ source: 'own', id: 'al-1', from: 'music' }, { source: 'own', id: 'al-2', from: 'music' }]);
  assert.deepEqual(titles(), ['Two']);
  assert.deepEqual(whole.map((x: { id: string }) => x.id), ['al-2'], 'what closes its panel');
});

test('what stays lists only the copies left', () => {
  library.view = view();
  copiesGone([{ source: 'own', id: 'al-1', from: 'music' }]);
  assert.deepEqual(librariesHeld('al-1'), ['mine']);
});

test('a copy gone from a list not read yet is a record gone: nothing says one stays', () => {
  library.view = null;
  assert.equal(copiesGone([{ source: 'own', id: 'al-1', from: 'music' }]).length, 1);
});

test('a moved copy keeps its record in the list, in its new library', () => {
  library.view = view();
  copiesMoved([{ source: 'own', id: 'al-1', from: 'music', to: 'more' }]);
  assert.deepEqual(titles(), ['Two', 'One']);
  assert.deepEqual(librariesHeld('al-1'), ['more', 'mine']);
});

test("what finds a record by its id alone sees the change: a search result's dots", () => {
  library.view = view();
  assert.deepEqual(libraryDots('own', { id: 'al-1' }).map((d: { name: string }) => d.name), ['music', 'mine']);
  copiesGone([{ source: 'own', id: 'al-1', from: 'music' }]);
  assert.deepEqual(libraryDots('own', { id: 'al-1' }).map((d: { name: string }) => d.name), ['mine']);
});
