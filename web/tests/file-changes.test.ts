// One way to change files (decision 0092; lib/fileChangeRules): a field's change as the review sheet says it, a run's
// line, the list's groups, a kept run taking its live one's place, the record's state line, and a row's name.
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { asideOf, changeOf, fieldLabel, rowLabel, runGroups, runLine, stateLine, withRun } = await import('../src/lib/fileChangeRules.ts');

const run = (o: Record<string, unknown>) => ({ id: 'r1', what: 'types', title: 'Types into files', state: 'done', done: 0, of: 0, failed: [], undone: false, albums: 1, ...o }) as never;

test('a change: added, taken away, or one for another', () => {
  assert.deepEqual(changeOf({ before: [], after: ['Classical'] }), { gone: [], came: ['Classical'], kind: 'add' });
  assert.deepEqual(changeOf({ before: ['Live'], after: [] }), { gone: ['Live'], came: [], kind: 'remove' });
  assert.deepEqual(changeOf({ before: ['Album'], after: ['EP'] }), { gone: ['Album'], came: ['EP'], kind: 'set' });
  assert.deepEqual(changeOf({ before: ['Jazz', 'Rock'], after: ['Jazz', 'Classical'] }), { gone: ['Rock'], came: ['Classical'], kind: 'set' }, 'a value kept is no change');
  assert.equal(fieldLabel('releasetype'), 'Type');
  assert.equal(fieldLabel('date'), 'Date');
  assert.equal(fieldLabel('mood'), 'Mood');
});

test("a run's line: waiting, writing n of m, set aside, undone", () => {
  assert.equal(runLine(run({ id: 'l1', state: 'waiting' })), 'Waiting');
  assert.equal(runLine(run({ id: 'l1', state: 'writing', done: 3, of: 12 })), 'Writing 3 of 12');
  assert.match(runLine(run({ at: 1_700_000_000, failed: [{ album: 'a', why: 'changed since it was planned' }] })), /^1 set aside · /);
  assert.match(runLine(run({ at: 1_700_000_000, undone: true })), /^Undone · /);
});

test('the list: Now first, then each day newest first', () => {
  const now = new Date(2026, 10, 1, 15, 0).getTime();
  const at = (d: number, h: number) => new Date(2026, 9, d, h).getTime() / 1000;
  const groups = runGroups(
    [run({ id: 'r1', at: new Date(2026, 10, 1, 9).getTime() / 1000 }), run({ id: 'l2', state: 'writing' }), run({ id: 'r3', at: at(31, 20) }), run({ id: 'r4', at: at(20, 10) }), run({ id: 'r5', at: new Date(2026, 10, 1, 14).getTime() / 1000 })],
    now,
  );
  assert.deepEqual(groups.map((g) => g.title).slice(0, 3), ['Now', 'Today', 'Yesterday']);
  assert.deepEqual(groups[1].runs.map((r: { id: string }) => r.id), ['r5', 'r1'], 'newest first');
  assert.equal(groups.length, 4, 'an older day by its date');
});

test('a kept run takes the place of its live one; news replaces by id', () => {
  const live = run({ id: 'l7', state: 'writing', what: 'types' });
  const other = run({ id: 'l8', state: 'waiting', what: 'renumber' });
  const kept = run({ id: 'r12', what: 'types', at: 5 });
  assert.deepEqual(withRun([live, other], kept).map((r: { id: string }) => r.id), ['r12', 'l8']);
  assert.deepEqual(withRun([kept], run({ id: 'r12', undone: true })).map((r: { undone: boolean }) => r.undone), [true]);
});

test("the record's state line: its words and what a tap does", () => {
  assert.equal(stateLine(null), null);
  assert.deepEqual(stateLine({ state: 'differs' }), { text: 'Its files differ from Cratarr', tone: null, act: 'review', progress: null });
  assert.equal(stateLine({ state: 'writing', done: 2, of: 4 })?.progress, 0.5);
  assert.equal(stateLine({ state: 'written', run: 'r3' })?.act, 'undo');
  assert.equal(stateLine({ state: 'written' })?.act, null, 'no run: nothing to undo');
  assert.equal(stateLine({ state: 'changed' })?.tone, 'warn');
});

test('what a run set aside: a row per record, its files counted, each reason once; the line counts records', () => {
  const failed = [
    { album: 'a', file: '01.flac', why: 'its audio could not be found' },
    { album: 'a', file: '02.flac', why: 'its audio could not be found' },
    { album: 'b', file: '01.mp3', why: 'the written fields did not read back as written', title: 'Elastic Variations', artist: 'Anthony Manning' },
    { album: 'c', why: 'changed since it was planned' },
  ];
  assert.deepEqual(asideOf(failed), [
    { album: 'a', title: undefined, artist: undefined, files: 2, why: ['its audio could not be found'] },
    { album: 'b', title: 'Elastic Variations', artist: 'Anthony Manning', files: 1, why: ['the written fields did not read back as written'] },
    { album: 'c', title: undefined, artist: undefined, files: 0, why: ['changed since it was planned'] },
  ]);
  assert.match(runLine(run({ at: 1_700_000_000, failed })), /^3 set aside · /);
});

test("a kept run under way again is its Undo: said so in the list and on the record's line", () => {
  assert.equal(runLine(run({ id: 'r4', state: 'waiting' })), 'Waiting to undo');
  assert.equal(runLine(run({ id: 'r4', state: 'writing', done: 2, of: 5 })), 'Undoing 2 of 5');
  assert.equal(runLine(run({ id: 'l4', state: 'writing', done: 2, of: 5 })), 'Writing 2 of 5');
  assert.equal(stateLine({ state: 'writing', done: 1, of: 4, run: 'r4' } as never)?.text, 'Undoing its files, 1 of 4');
  assert.equal(stateLine({ state: 'waiting', run: 'l4' } as never)?.text, 'Waiting to write its files');
});

test("a Keep's or a move's files moved: Adding to its library, where its title says", async () => {
  const { runLine } = await import('../src/lib/fileChangeRules.ts');
  const run = { id: 'l1', what: 'keep', title: 'Kept into Music: Ignorance', state: 'adding' as const, done: 1, of: 1, failed: [], undone: false, albums: 1 };
  assert.equal(runLine(run), 'Adding to Music…');
  assert.equal(runLine({ ...run, title: 'Replaced: Ignorance' }), 'Adding to its library…');
});

// As the Library's list has them; the server's word for a library on a sheet is the same folder's path.
const LIBRARIES = [
  { id: '/music', name: 'Music' },
  { id: '/incoming', name: 'Incoming' },
  { id: '/own/someone', name: 'Your music' },
];

test("a cover on two copies: its rows no longer read alike, each saying its library by name", () => {
  assert.equal(rowLabel({ field: 'cover', library: '/music' }, LIBRARIES), 'Cover · Music');
  assert.equal(rowLabel({ field: 'cover', library: '/incoming' }, LIBRARIES), 'Cover · Incoming');
  assert.equal(rowLabel({ field: 'cover', library: '/own/someone' }, LIBRARIES), 'Cover · Your music', "the viewer's own, as the list names it");
});

test("a track whose copies differ: its library after the track's name", () => {
  assert.equal(rowLabel({ field: 'bpm', track: 'One', library: '/music' }, LIBRARIES), 'One · Music');
  assert.equal(rowLabel({ field: 'bpm', track: 'One', library: '/incoming' }, LIBRARIES), 'One · Incoming');
});

test('a row with no library reads as it did', () => {
  assert.equal(rowLabel({ field: 'cover' }, LIBRARIES), 'Cover');
  assert.equal(rowLabel({ field: 'bpm', track: 'One' }, LIBRARIES), 'One');
});

test("a library the page has no name for goes unsaid: a row never shows a folder's path", () => {
  assert.equal(rowLabel({ field: 'cover', library: '/elsewhere' }, LIBRARIES), 'Cover');
  assert.equal(rowLabel({ field: 'cover', library: '/music' }, []), 'Cover', 'the list not read yet');
});

// The sheet does not run here, so this reads it.
test("the sheet names its rows by that rule, from the Library's list, read there if nothing has yet", () => {
  const sheet = readFileSync(new URL('../src/lib/FileChanges.svelte', import.meta.url), 'utf8');
  assert.match(sheet, /\{rowLabel\(f, libraries\)\}/);
  assert.doesNotMatch(sheet, /f\.track \?\? fieldLabel/);
  assert.match(sheet, /const libraries = \$derived\(library\.view\?\.sources\.find\(\(s\) => s\.id === OWN\)\?\.library_info \?\? \[\]\);/);
  assert.match(sheet, /if \(!library\.view\) readLibrary\(false\)/);
});
