// Where a track asked from at or past its end plays from (lib/player/resumePlace): the next track, the same one again
// under repeat one, round to the first under repeat all, the queue's last standing at its start; a place inside the
// track as asked.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { resumePlace, PAST_END_S } = await import('../src/lib/player/resumePlace.ts');

test('a place inside the track plays as asked', () => {
  assert.deepEqual(resumePlace(1, 120, 264, 3, 'off', true), { index: 1, from: 120, play: true });
  assert.deepEqual(resumePlace(1, 264 - PAST_END_S - 0.1, 264, 3, 'off', false), { index: 1, from: 264 - PAST_END_S - 0.1, play: false });
});

test('past the end (the 0.15 desktop app: 4:45 of a 4:24 track, then 4:24 itself): the next track from its start', () => {
  assert.deepEqual(resumePlace(1, 285, 264, 3, 'off', true), { index: 2, from: 0, play: true });
  assert.deepEqual(resumePlace(1, 264, 264, 3, 'off', true), { index: 2, from: 0, play: true });
  assert.deepEqual(resumePlace(1, 263.5, 264, 3, 'off', true), { index: 2, from: 0, play: true });
});

test('the queue\'s last past its end stands at its start; repeat one plays it again; repeat all goes round', () => {
  assert.deepEqual(resumePlace(2, 300, 264, 3, 'off', true), { index: 2, from: 0, play: false });
  assert.deepEqual(resumePlace(2, 300, 264, 3, 'one', true), { index: 2, from: 0, play: true });
  assert.deepEqual(resumePlace(1, 300, 264, 3, 'one', true), { index: 1, from: 0, play: true });
  assert.deepEqual(resumePlace(2, 300, 264, 3, 'all', true), { index: 0, from: 0, play: true });
});

test('a length not known keeps the place (the stream then says); no place, or a wrong one, is the start', () => {
  assert.deepEqual(resumePlace(0, 285, null, 3, 'off', true), { index: 0, from: 285, play: true });
  assert.deepEqual(resumePlace(0, 285, undefined, 3, 'off', true), { index: 0, from: 285, play: true });
  assert.deepEqual(resumePlace(0, -3, 264, 3, 'off', true), { index: 0, from: 0, play: true });
  assert.deepEqual(resumePlace(0, Number.NaN, 264, 3, 'off', true), { index: 0, from: 0, play: true });
});

test('a track shorter than the margin still plays from its start', () => {
  assert.deepEqual(resumePlace(1, 0, 0.6, 3, 'off', true), { index: 1, from: 0, play: true });
});
