// Which file the tape head reads (lib/tape/file.ts, decision 0102): a track's, and under Play from the copy it names, as the
// app's HeadKeyTest says of Tracks.kt. Known by the track alone, the head would play the copy it holds.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { headFile, headKey } = await import('../src/lib/tape/file.ts');

const best = { source: 'cratarr', id: 'tr-1' };
const music = { ...best, from: 'cratarr//media/Music' };
const incoming = { ...best, from: 'cratarr//r&b new/in' };

test('a track with no copy named is known by its source and id', () => {
  assert.equal(headKey(best), 'cratarr/tr-1');
});

test('a copy named is another file of the same track, in the app\'s shape', () => {
  assert.equal(headKey(music), 'cratarr/tr-1@cratarr//media/Music');
  assert.notEqual(headKey(music), headKey(best));
  assert.notEqual(headKey(music), headKey(incoming));
});

test('a track with no copy named asks for its file as it is', () => {
  assert.equal(headFile(best), '/api/file/cratarr/tr-1');
});

test('the file is asked of the copy named, a library\'s id arriving whole, whatever it holds', () => {
  const asked = new URL(headFile(incoming), 'http://cratarr.test');
  assert.equal(asked.pathname, '/api/file/cratarr/tr-1');
  assert.equal(asked.searchParams.get('from'), 'cratarr//r&b new/in');
});

test('a source and an id are one part of the address each, whatever they hold', () => {
  assert.equal(headFile({ source: 'a/b', id: 'c d' }), '/api/file/a%2Fb/c%20d');
});
