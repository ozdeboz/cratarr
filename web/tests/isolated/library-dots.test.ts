// A record's dots wherever only its id is known (lib/albums: a search result, the panel's header): its copies come from
// the Library's list, as last read, and the switches are the page's.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { library, libraryDots } = await import('../../src/lib/albums.svelte.ts');
const { prefs } = await import('../../src/lib/prefs.svelte.ts');

const lib = (id: string, kind: string) => ({ id, name: id, kind, colour: `#${id}` });
const view = (albums: unknown[]) =>
  ({ sources: [{ id: 'own', albums, library_info: [lib('music', 'main'), lib('mine', 'own'), lib('in', 'incoming')] }], trash_days: 7, music: null }) as never;
const names = (a: { id: string; library?: string }) => libraryDots('own', a).map((d: { name: string }) => d.name);

test('no dots until the Library\'s list is read, nor from a server it does not hold', () => {
  library.view = null;
  assert.deepEqual(names({ id: 'al-1', library: 'music' }), []);
  library.view = view([{ id: 'al-1', title: 'One', library: 'music' }]);
  assert.deepEqual(libraryDots('other', { id: 'al-1' }), []);
});

test('an id alone finds the copies the list gives it, in the list\'s order', () => {
  library.view = view([{ id: 'al-1', title: 'One', copies: [{ library: 'mine' }, { library: 'music' }] }]);
  assert.deepEqual(names({ id: 'al-1' }), ['mine', 'music']);
});

test('an id alone finds the one library of a list that names no copies', () => {
  library.view = view([{ id: 'al-2', title: 'Two', library: 'in' }]);
  assert.deepEqual(names({ id: 'al-2' }), ['in']);
});

test('what a card says of itself goes before the list', () => {
  library.view = view([{ id: 'al-3', title: 'Three', library: 'music' }]);
  assert.deepEqual(names({ id: 'al-3', library: 'mine' }), ['mine']);
});

test('an id the list does not hold has no dots', () => {
  library.view = view([]);
  assert.deepEqual(names({ id: 'al-4' }), []);
});

test('a switch thrown on the page takes its copy\'s dot at once', () => {
  library.view = view([{ id: 'al-5', title: 'Five', copies: [{ library: 'mine' }, { library: 'music' }] }]);
  prefs.hiddenLibraries = ['own/music'];
  assert.deepEqual(names({ id: 'al-5' }), ['mine']);
  prefs.hiddenLibraries = ['own/mine'];
  assert.deepEqual(names({ id: 'al-5' }), [], 'home alone on goes without saying');
  prefs.hiddenLibraries = [];
});
