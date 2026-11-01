// The Library's release types in three layers (lib/recordTypeRules; the owner, 2026-10-31): the picker offers the Type
// axis as the server counts it, reads the Type rows only (one type's name, else Selected types), a pick replaces them;
// rows saved before the layers read once as the layers have them; the Filter sheet says "or" within the release types.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { ALL, SELECTED, layerIn, layeredRows, recordWord, recordsTitle, recordsWord, rowsOfType, typeLabel, typeOfRows, typeOptions, typesSaid, typesShown } = await import('../src/lib/recordTypeRules.ts');

const row = (values: string[], not = false) => ({ axis: 'type', not, value: { kind: 'any_of' as const, values } });

test("the picker offers Records, then the axis's types with records, in its order, with their counts", () => {
  const options = typeOptions({ values: ['Album', 'Live', 'Classical', 'Broadcast', 'Single'], counts: [2100, 230, 180, 0, 95] });
  assert.deepEqual(options, [
    { id: ALL, label: 'Records' },
    { id: 'Album', label: 'Albums', count: 2100 },
    { id: 'Live', label: 'Live', count: 230 },
    { id: 'Classical', label: 'Classical', count: 180 },
    { id: 'Single', label: 'Singles', count: 95 },
  ]);
  assert.deepEqual(typeOptions(null), [{ id: ALL, label: 'Records' }], 'before the axis: Records only');
});

test('types named in the plural where it reads so, else as they are', () => {
  assert.deepEqual(['Album', 'EP', 'Single', 'Broadcast', 'Compilation', 'DJ-mix', 'Mixtape/Street', 'Spokenword', 'Audio drama', 'Live', 'Other', 'Classical', 'Something new'].map(typeLabel), [
    'Albums', 'EPs', 'Singles', 'Broadcasts', 'Compilations', 'DJ-mixes', 'Mixtapes', 'Spoken word', 'Audio dramas', 'Live', 'Other', 'Classical', 'Something new',
  ]);
});

test('the label reads the Type rows only: none Records, one plain value its name, anything else Selected types', () => {
  assert.equal(typesShown([]), 'Records');
  assert.equal(typesShown([row(['Album'])]), 'Albums');
  assert.equal(typesShown([row(['Live'])]), 'Live');
  assert.equal(typesShown([row(['Album', 'EP'])]), SELECTED);
  assert.equal(typesShown([row(['Compilation'], true)]), SELECTED, 'Not');
  assert.equal(typesShown([row(['Album']), row(['Live'])]), SELECTED, 'two rows');
  assert.equal(typesShown([{ axis: 'type', not: false, value: { kind: 'all_of', values: ['Live'] } }]), SELECTED);
  assert.equal(typeOfRows([row(['Album', 'EP'])]), null, 'Selected types ticks nothing');
});

test('a pick is its one row, Records none, and reads back as itself', () => {
  assert.deepEqual(rowsOfType(ALL), []);
  assert.deepEqual(rowsOfType('Live'), [row(['Live'])]);
  for (const t of [ALL, 'Album', 'Live', 'Classical']) assert.equal(typeOfRows(rowsOfType(t)), t);
});

test('rows saved before the layers: Incomplete leaves them, the old Other becomes any of Other', () => {
  const year = { axis: 'year', not: false, value: { kind: 'range' as const, min: 1990, max: null } };
  assert.deepEqual(layeredRows([row(['Incomplete'])]), []);
  assert.deepEqual(layeredRows([row(['Album', 'Incomplete']), year]), [row(['Album']), year]);
  assert.deepEqual(layeredRows([row(['Album', 'EP', 'Single', 'Compilation', 'Incomplete', 'Classical'], true)]), [row(['Other'])]);
  assert.deepEqual(layeredRows([row(['Album', 'EP', 'Single', 'Compilation'], true)]), [row(['Other'])], 'the Other from before Incomplete');
  assert.deepEqual(layeredRows([row(['Compilation'], true)]), [row(['Compilation'], true)], 'a Not of its own stays');
  assert.deepEqual(layeredRows([row(['Live'])]), [row(['Live'])]);
});

test('the Filter sheet says "or" within a layer and "and" across, as the server reads them', () => {
  const axis = { values: ['Album', 'Live', 'Classical', 'EP', 'Compilation'], layers: [1, 2, 3, 1, 2] };
  const said = (v: string[]) => typesSaid(v, layerIn(axis));
  assert.equal(said(['Album', 'EP']), 'Album or EP');
  assert.equal(said(['Album', 'Live']), 'Album and Live');
  assert.equal(said(['Live', 'Album', 'Classical', 'EP']), 'Album or EP and Live and Classical');
  assert.equal(said(['Compilation', 'Live']), 'Compilation or Live', 'one layer: either');
  assert.equal(typesSaid(['Album', 'EP'], layerIn(null)), 'Album and EP', 'no layers known: each its own');
});

test('records are named by their one release type, else records (the owner, 2026-11-01)', () => {
  assert.equal(recordWord('ep'), 'EP');
  assert.equal(recordWord('album'), 'album');
  assert.equal(recordWord('other'), 'record', 'Other names no kind');
  assert.equal(recordWord(null), 'record');
  assert.equal(recordsWord(['single', 'single']), 'single');
  assert.equal(recordsWord(['album', 'ep']), 'record');
  assert.equal(recordsWord(['album', undefined]), 'record', 'one without a type: records');
  assert.equal(recordsWord([]), 'record');
  assert.equal(recordsTitle(['ep']), 'EPs');
  assert.equal(recordsTitle(['album', 'album']), 'Albums');
  assert.equal(recordsTitle(['album', 'single']), 'Records');
});
