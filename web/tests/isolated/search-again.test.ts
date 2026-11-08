// A search as reads move (lib/globalSearch, the real one): what it holds came from the other place, or failed on the way there.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { globalSearch } = await import('../../src/lib/globalSearch.svelte.ts');
const { whenReadsMove } = await import('../../src/lib/deviceIndex.svelte.ts');

test('reads moving asks the words again, of where reads now come from', () => {
  const was = globalSearch.again;
  for (const moved of whenReadsMove) moved();
  assert.equal(globalSearch.again, was + 1);
});
