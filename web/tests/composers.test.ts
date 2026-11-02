// The composers (lib/composers): by their sort name, and in the order chosen.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { byComposer, composersInOrder } = await import('../src/lib/composers.ts');

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
