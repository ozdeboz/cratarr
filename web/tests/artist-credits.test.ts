// An artist's credits and key (lib/artistCredits, decision 0083): a track's artist line split as the server splits it,
// whole names only, and the loose key it keeps them by.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { artistKey, creditsOf } = await import('../src/lib/artistCredits.ts');

test('a credit split into its artists, each once, whole names only', () => {
  assert.deepEqual(creditsOf('Air feat. Beck & Jarvis Cocker'), ['Air', 'Beck', 'Jarvis Cocker']);
  assert.deepEqual(creditsOf('Bonobo, Totally Enormous Extinct Dinosaurs'), ['Bonobo', 'Totally Enormous Extinct Dinosaurs']);
  assert.deepEqual(creditsOf('Skrillex x Fred again..'), ['Skrillex', 'Fred again..']);
  assert.deepEqual(creditsOf('Becky Hill'), ['Becky Hill'], 'no Beck inside Becky');
  assert.deepEqual(creditsOf('Air ft. AIR'), ['Air'], 'once');
  assert.deepEqual(creditsOf('AC/DC'), ['AC/DC'], 'a slash is part of a name');
  assert.deepEqual(creditsOf(null), []);
});

test('the key: case and accents aside', () => {
  assert.equal(artistKey('  Björk '), 'bjork');
  assert.equal(artistKey('AIR'), artistKey('Air'));
});
