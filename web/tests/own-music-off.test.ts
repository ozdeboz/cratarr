// Own music per account, on or off (lib/ownMusic, Settings › Libraries; the owner, 2026-11-08): the accounts it is switched off for,
// as Cratarr's own source keeps them.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { ownMusicOff, withOwnMusic } = await import('../src/lib/ownMusic.ts');

test('own music is on for every account until switched off, and back on', () => {
  assert.deepEqual(ownMusicOff({}), [], 'nothing kept: on for everyone');
  assert.deepEqual(ownMusicOff({ own_music_off: ['bo', 3] }), ['bo'], 'only names');
  const off = withOwnMusic([], 'bo', false);
  assert.deepEqual(off, ['bo']);
  assert.deepEqual(withOwnMusic(off, 'bo', false), ['bo'], 'once');
  assert.deepEqual(withOwnMusic(off, 'bo', true), []);
  assert.deepEqual(withOwnMusic(off, 'ann', true), ['bo'], 'another left as it is');
});
