// The record kinds (lib/recordKindRules; 0.10: Classical): offered in the owner's order where the server knows
// Classical, each the filter's Type row it is, a row read back as its kind; Other as none of the named; the composers
// by their sort name.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { byComposer, kindOfRows, kindsOffered, rowsOfKind } = await import('../src/lib/recordKindRules.ts');

test('Classical between Compilations and Other, offered only where the server knows it', () => {
  assert.deepEqual(kindsOffered(true).map((k) => k.label), ['Records', 'Albums', 'EPs', 'Singles', 'Compilations', 'Classical', 'Other']);
  assert.deepEqual(kindsOffered(false).map((k) => k.label), ['Records', 'Albums', 'EPs', 'Singles', 'Compilations', 'Other']);
});

test('each kind is its Type row, and reads back as itself', () => {
  for (const classical of [true, false]) {
    for (const k of kindsOffered(classical)) assert.equal(kindOfRows(rowsOfKind(k.id, classical), classical), k.id, `${k.id} (${classical})`);
  }
  assert.deepEqual(rowsOfKind('Classical', true), [{ axis: 'type', not: false, value: { kind: 'any_of', values: ['Classical'] } }]);
  assert.deepEqual(rowsOfKind('other', true)[0].value, { kind: 'any_of', values: ['Album', 'EP', 'Single', 'Compilation', 'Classical'] });
  assert.deepEqual(rowsOfKind('other', false)[0].value, { kind: 'any_of', values: ['Album', 'EP', 'Single', 'Compilation'] });
});

test('an Other row made before Classical still reads as Other; rows of no kind read as none', () => {
  assert.equal(kindOfRows(rowsOfKind('other', false), true), 'other');
  assert.equal(kindOfRows([{ axis: 'type', not: false, value: { kind: 'any_of', values: ['Album', 'EP'] } }], true), null);
  assert.equal(kindOfRows([{ axis: 'type', not: false, value: { kind: 'any_of', values: ['Classical'] } }], false), null, 'not offered: none of them');
  assert.equal(kindOfRows([], true), 'all');
});

test('composers by their sort name, accents and case aside', () => {
  const c = (name: string, sort_name?: string) => ({ key: name, name, sort_name, records: 1 });
  const list = [c('Johann Sebastian Bach', 'Bach, Johann Sebastian'), c('Béla Bartók', 'Bartók, Béla'), c('Arvo Pärt', 'Pärt, Arvo'), c('anonymous')];
  assert.deepEqual([...list].sort(byComposer).map((x) => x.name), ['anonymous', 'Johann Sebastian Bach', 'Béla Bartók', 'Arvo Pärt']);
});
