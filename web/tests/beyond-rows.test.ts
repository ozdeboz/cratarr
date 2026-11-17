// Beyond in the search (W12, C57; lib/beyondRows): each row by its record, else its release group, else its Discogs master; left
// out where the library has it or what Cratarr knows lists it already by record or group, never by its name.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { beyondFound, beyondKey, beyondSubject, knownOf } = await import('../src/lib/beyondRows.ts');

const row = (r: Record<string, unknown>) => ({ title: 'Further', artist: 'GROUCH', ...r });

test("each row's subject: its record, else its release group, else its Discogs master", () => {
  assert.equal(beyondSubject(row({ record: 'al-88', release_group: 'g1', discogs: 4 })), 'beyond:al-88');
  assert.equal(beyondSubject(row({ release_group: 'g1', discogs: 4 })), 'beyond:g1');
  assert.equal(beyondKey(row({ discogs: 4 })), 'd4');
});

test('a row listed already by its record or its group is left out; one of the same name is not', () => {
  const known = knownOf(['album:cratarr/al-88', 'spotify:g2', 'missing:x']);
  const kept = beyondFound([row({ record: 'al-88' }), row({ release_group: 'g2' }), row({ release_group: 'g3' }), row({ discogs: 7 })], known);
  assert.deepEqual(kept.map(beyondKey), ['g3', 'd7'], 'the same name, another album: kept');
});

test('a row the library has is left out, and each row is shown once', () => {
  const kept = beyondFound([row({ release_group: 'g1', have: true }), row({ release_group: 'g2' }), row({ release_group: 'g2', discogs: 9 })], knownOf([]));
  assert.deepEqual(kept.map(beyondKey), ['g2']);
});
