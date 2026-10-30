// A wind only inside its commitment (lib/player/windBound): begun, committed to playing; a ramp ended outright while
// it sounds (a new queue, a Stop, a seek during a ramp) ends the wind through its own end first.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { rampSlot } = await import('../src/lib/player/rampSlot.ts');
const { motionSlot } = await import('../src/lib/player/motionSlot.ts');
const { windBound } = await import('../src/lib/player/windBound.ts');

function deck() {
  const ramp = rampSlot();
  const motion = motionSlot<string, never, never>();
  const ended: string[] = [];
  // endRewind: the wind gone, then its commitment.
  const b = windBound(ramp, motion, () => {
    ended.push(motion.rewinding!);
    motion.rewinding = null;
    ramp.committed = null;
  });
  return { ramp, motion, b, ended };
}

test('a wind begun is committed to playing at once', () => {
  const t = deck();
  t.b.wind('w');
  assert.equal(t.ramp.committed, false);
  t.b.commit(false);
  assert.deepEqual(t.ended, []);
});

test('a new queue, a Stop or a seek during a ramp, while a wind sounds: the wind ends through its own end, then the ramp', () => {
  for (const what of ['queue', 'stop', 'seek']) {
    const t = deck();
    t.b.wind(what);
    t.b.commit(null);
    assert.deepEqual(t.ended, [what], `${what}: its clip stopped`);
    assert.equal(t.motion.rewinding, null);
    assert.equal(t.ramp.committed, null);
  }
});

test("a wind's own end (endRewind) runs once: the wind gone before its commitment", () => {
  const t = deck();
  t.b.wind('w');
  t.b.wind(null);
  t.b.commit(null);
  assert.deepEqual(t.ended, [], 'already over: nothing to end');
});

test('a pause turning a ramp round keeps the wind out of it: no wind, nothing ended', () => {
  const t = deck();
  t.b.commit(false);
  t.b.commit(true);
  t.b.commit(null);
  assert.deepEqual(t.ended, []);
});
