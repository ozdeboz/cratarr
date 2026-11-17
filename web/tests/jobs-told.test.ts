// Who is told of a finished conversion (W13, X5; lib/jobsTold): an administrator of every one, a user only of those their own
// page queued, each with its Undo, which is theirs to use.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { queuedHere, toldOf } = await import('../src/lib/jobsTold.ts');

const job = (album_id: string, from: string | null = 'incoming', undo = `u-${album_id}`) => ({ source: 'cratarr', album_id, from, undo, title: album_id });

test('an administrator is told of every finished conversion', () => {
  assert.deepEqual(toldOf([job('al-1'), job('al-2')], true, []).map((j) => j.album_id), ['al-1', 'al-2']);
});

test('a user only of the conversions their page queued, each with its own Undo', () => {
  const told = toldOf([job('al-1'), job('al-2'), job('al-3')], false, [{ source: 'cratarr', album_id: 'al-2', from: 'incoming' }]);
  assert.deepEqual(told.map((j) => j.undo), ['u-al-2']);
  assert.deepEqual(toldOf([job('al-1')], false, []), [], 'nothing queued here: nothing told');
});

test('a job is known by its record, and by its copy where both name one', () => {
  assert.ok(queuedHere([{ source: 'cratarr', album_id: 'al-2' }], job('al-2', 'incoming')));
  assert.ok(queuedHere([{ source: 'cratarr', album_id: 'al-2', from: 'incoming' }], job('al-2', null)));
  assert.ok(!queuedHere([{ source: 'cratarr', album_id: 'al-2', from: 'music' }], job('al-2', 'incoming')), 'another copy of it');
  assert.ok(!queuedHere([{ source: 'other', album_id: 'al-2' }], job('al-2')), 'another server');
});
