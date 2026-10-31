// The record kinds (lib/recordKindRules; 0.10: Classical): offered in the owner's order where the server knows
// Classical, each the filter's Type row it is, a row read back as its kind; Other as none of the named; the composers
// by their sort name.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { byComposer, composersInOrder, kindOfRows, kindsOffered, rowsOfKind } = await import('../src/lib/recordKindRules.ts');

test('Classical between Compilations and Incomplete, offered only where the server knows it; Incomplete before Other', () => {
  assert.deepEqual(kindsOffered(true).map((k) => k.label), ['Records', 'Albums', 'EPs', 'Singles', 'Compilations', 'Classical', 'Incomplete', 'Other']);
  assert.deepEqual(kindsOffered(false).map((k) => k.label), ['Records', 'Albums', 'EPs', 'Singles', 'Compilations', 'Incomplete', 'Other']);
});

test('each kind is its Type row, and reads back as itself', () => {
  for (const classical of [true, false]) {
    for (const k of kindsOffered(classical)) assert.equal(kindOfRows(rowsOfKind(k.id, classical), classical), k.id, `${k.id} (${classical})`);
  }
  assert.deepEqual(rowsOfKind('Classical', true), [{ axis: 'type', not: false, value: { kind: 'any_of', values: ['Classical'] } }]);
  assert.deepEqual(rowsOfKind('other', true)[0].value, { kind: 'any_of', values: ['Album', 'EP', 'Single', 'Compilation', 'Incomplete', 'Classical'] });
  assert.deepEqual(rowsOfKind('other', false)[0].value, { kind: 'any_of', values: ['Album', 'EP', 'Single', 'Compilation', 'Incomplete'] });
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

test('the composers in the order chosen: by name, or by how many records, as many by name; turned round', () => {
  const c = (name: string, records: number, sort_name: string | null = null) => ({ key: name, name, sort_name, records });
  const list = [c('Johannes Brahms', 4, 'Brahms, Johannes'), c('Johann Sebastian Bach', 9, 'Bach, Johann Sebastian'), c('Érik Satie', 4, 'Satie, Erik'), c('Anon', 1)];
  const names = (l: { name: string }[]) => l.map((x) => x.name);
  assert.deepEqual(names(composersInOrder(list, 'name', false)), ['Anon', 'Johann Sebastian Bach', 'Johannes Brahms', 'Érik Satie']);
  assert.deepEqual(names(composersInOrder(list, 'name', true)), ['Érik Satie', 'Johannes Brahms', 'Johann Sebastian Bach', 'Anon']);
  // Fewest first; turned round most first; Brahms and Satie (4 each) by name both ways.
  assert.deepEqual(names(composersInOrder(list, 'records', false)), ['Anon', 'Johannes Brahms', 'Érik Satie', 'Johann Sebastian Bach']);
  assert.deepEqual(names(composersInOrder(list, 'records', true)), ['Johann Sebastian Bach', 'Johannes Brahms', 'Érik Satie', 'Anon']);
  // The list given is left as it was.
  assert.equal(list[0].name, 'Johannes Brahms');
});
