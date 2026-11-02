// A track asked of the player elsewhere (lib/player/asked; the owner, 2026-11-02: the phone driving the tablet, a row
// tapped flipped between it and the track before): reports from before it are not taken, its own is, and past
// ASKED_MS the player's word stands.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ask, ASKED_MS, heard } from '../src/lib/player/asked.ts';

const queue = ['a', 'b', 'c', 'b'];
const keyAt = (i: number) => queue[i] ?? null;

test('nothing asked: every report is taken', () => {
  assert.deepEqual(heard(null, 0, keyAt, 0), { asked: null, before: false });
});

test('the track before, again and again: not taken until the player is on the one asked', () => {
  let a = ask('c', 2, 1000);
  for (const t of [1100, 1400, 2500]) {
    const v = heard(a, 0, keyAt, t);
    assert.equal(v.before, true, `at ${t}`);
    a = v.asked;
  }
  const on = heard(a, 2, keyAt, 2600);
  assert.deepEqual(on, { asked: null, before: false });
  assert.equal(heard(on.asked, 0, keyAt, 2700).before, false, 'then the player is believed again');
});

test('the same track elsewhere in the queue is not the one asked', () => {
  assert.equal(heard(ask('b', 3, 0), 1, keyAt, 10).before, true);
});

test("past its time the player's word stands", () => {
  const a = ask('c', 2, 0);
  assert.equal(heard(a, 0, keyAt, ASKED_MS - 1).before, true);
  assert.deepEqual(heard(a, 0, keyAt, ASKED_MS + 1), { asked: null, before: false });
});

test('a report of the queue alone (no index) is from before too', () => {
  assert.equal(heard(ask('c', 2, 0), undefined, keyAt, 10).before, true);
});
