// A conversion works on one copy of a record (`JobView.from`): the other copies' entries stay, and the row still shows it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const { jobs } = await import('../../src/lib/jobs.svelte.ts');

const job = (more: object = {}) => ({ id: 7, source: 'own', album_id: 'al-1', title: 'One', action: 'keep', state: 'converting', done: 0, part: 0, total: 3, folder: 'One', undo: null, why: null, from: 'in', ...more }) as never;

test("a record's job is its Incoming copy's, not its kept copy's", () => {
  jobs.apply([job()]);
  assert.equal(jobs.of('own', 'al-1', 'in')?.id, 7);
  assert.equal(jobs.of('own', 'al-1', 'music'), undefined);
});

test("a row's meter shows whichever copy converts", () => {
  jobs.apply([job()]);
  assert.equal(jobs.of('own', 'al-1')?.id, 7);
});

test('a job done is no job, and another record has none', () => {
  jobs.apply([job({ state: 'done' })]);
  assert.equal(jobs.of('own', 'al-1', 'in'), undefined);
  jobs.apply([job()]);
  assert.equal(jobs.of('own', 'al-2', 'in'), undefined);
});

test('a job that does not say where it comes from holds every copy back, the safe side', () => {
  jobs.apply([job({ from: undefined })]);
  assert.equal(jobs.of('own', 'al-1', 'music')?.id, 7);
});

const read = (f: string) => readFileSync(new URL(`../../src/${f}`, import.meta.url), 'utf8');

test('nothing takes a copy leaving for the record gone: only `copiesGone` says when a panel closes', () => {
  for (const f of ['lib/AlbumMenu.svelte', 'lib/SeveralAlbums.svelte']) assert.match(read(f), /of copiesGone\(.*albumGone\(x\.source, x\.id\)/, f);
  assert.doesNotMatch(read('lib/AlbumMenu.svelte'), /^\s*albumGone\(/m);
  assert.doesNotMatch(read('Library.svelte'), /albums\.filter\(\(x\) => x\.id !== id\)/, 'the row went, to come back with the next read');
});

test("a menu's offers ask each copy's own job", () => {
  assert.match(read('lib/AlbumMenu.svelte'), /\(library\) => jobs\.of\(s\.id, a\.id, library\)/);
});
