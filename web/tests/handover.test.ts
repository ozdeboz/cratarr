// No frame of the page between two places of one thing (the owner, 2026-10-31): while what comes gets ready, what goes
// stays drawn, for every move between beside the page, over it and over everything; opening and closing are no
// hand-over, and coming back to a place still drawn waits for nothing.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { nextHeld, drawn, handing, KEYS, NONE } = await import('../src/lib/handover.ts');
type P = { beside: boolean; over: boolean; overlay: boolean };

const all: P[] = [];
for (let n = 0; n < 8; n++) all.push({ beside: !!(n & 1), over: !!(n & 2), overlay: !!(n & 4) });
const any = (p: P) => KEYS.some((k) => p[k]);
const B = { ...NONE, beside: true };
const O = { ...NONE, over: true };
const V = { ...NONE, overlay: true };

test('every move, from any hand-over under way, keeps all that was drawn until what comes shows', () => {
  for (const held0 of all)
    for (const from of all)
      for (const to of all) {
        const was = drawn(from, held0);
        const held = nextHeld(held0, from, to);
        const comes = KEYS.some((k) => to[k] && !was[k]);
        const during = drawn(to, held);
        // Something new is getting ready: nothing drawn before goes yet.
        if (comes) for (const k of KEYS) if (was[k]) assert.ok(during[k], `${JSON.stringify([held0, from, to])}: ${k} let go too soon`);
        // Nothing new: nothing waits (there is no first frame to wait for), so nothing is left over the page.
        if (!comes) assert.equal(handing(held), false);
        // Only places not asked for are held.
        for (const k of KEYS) assert.ok(!(held[k] && to[k]));
      }
});

test('beside to over the page (the reported case) and back; the player maximised and back', () => {
  assert.deepEqual(nextHeld(NONE, B, O), B);
  assert.deepEqual(nextHeld(NONE, O, B), O);
  assert.deepEqual(nextHeld(NONE, B, V), B);
  assert.deepEqual(nextHeld(NONE, V, B), V);
  assert.deepEqual(nextHeld(NONE, O, V), O);
});

test('opening from nothing and closing to nothing hold nothing', () => {
  for (const p of all.filter(any)) {
    assert.deepEqual(nextHeld(NONE, NONE, p), NONE);
    assert.deepEqual(nextHeld(NONE, p, NONE), NONE);
  }
});

test('a second move before the first has shown keeps both; back to the held place, nothing waits', () => {
  const a = nextHeld(NONE, B, O);
  assert.deepEqual(nextHeld(a, O, V), { ...NONE, beside: true, over: true });
  assert.deepEqual(nextHeld(a, O, B), NONE);
});
