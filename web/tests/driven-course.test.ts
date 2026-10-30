// A player driven through the server as the page counts it on (lib/drivenCourse; the owner, 2026-10-30: the tablet
// driving the ThinkPad "doesn't show reality", gliding to a wind's first report and standing): its speed as it says
// it, else 1 playing and 0 paused, and through a wind as measured between its reports; then counted on from when each
// place was so (`position_at`, read as the app's deckAtMs is), each correction fading in. A host winding at 8× and
// reporting every 250 ms, its reports reaching the page late by a varying amount: the place shown stays near the
// truth, never stands still between reports, never jumps.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { speedBetween, measure, speedOf, MEASURE_MAX_MS, STAND_MS } = await import('../src/lib/drivenCourse.ts');
const { shownAt, easeInto, readAt } = await import('../src/lib/deckCourse.ts');
type Course = import('../src/lib/deckCourse.ts').Course;
type Ease = import('../src/lib/deckCourse.ts').Ease;
type Measured = import('../src/lib/drivenCourse.ts').Measured;

test('the speed between two places of a track, and none across a track or too long a gap', () => {
  assert.equal(speedBetween({ index: 0, pos: 10, at: 1000 }, { index: 0, pos: 12, at: 1250 }), 8);
  assert.equal(speedBetween({ index: 0, pos: 12, at: 1000 }, { index: 0, pos: 10, at: 1250 }), -8);
  assert.equal(speedBetween({ index: 0, pos: 10, at: 1000 }, { index: 1, pos: 0.5, at: 1250 }), null);
  assert.equal(speedBetween({ index: 0, pos: 10, at: 1000 }, { index: 0, pos: 30, at: 1000 + MEASURE_MAX_MS + 1 }), null);
  assert.equal(speedBetween({ index: 0, pos: 10, at: 1000 }, { index: 0, pos: 10.1, at: 1010 }), null);
  assert.equal(speedBetween(null, { index: 0, pos: 10, at: 1000 }), null);
});

test('the same report sent again later (another player changed) keeps when it was so and its speed', () => {
  let m: Measured | null = null;
  m = measure(m, { index: 0, pos: 10, at: 1000 });
  m = measure(m, { index: 0, pos: 12, at: 1250 });
  assert.equal(m.v, 8);
  // The server stamps the view as it sends it: the same place, a later stamp.
  const again = measure(m, { index: 0, pos: 12, at: 1250 + STAND_MS - 10 });
  assert.equal(again, m);
  assert.equal(again.seen.at, 1250);
  assert.equal(again.v, 8);
});

test('the same place for longer than a report interval: it stands, from when it was first there', () => {
  let m: Measured | null = null;
  m = measure(m, { index: 0, pos: 10, at: 1000 });
  m = measure(m, { index: 0, pos: 12, at: 1250 });
  m = measure(m, { index: 0, pos: 12, at: 1250 + STAND_MS + 10 });
  assert.equal(m.v, 0);
  assert.equal(m.seen.at, 1250);
  // On again from there: measured from the place it stood at.
  m = measure(m, { index: 0, pos: 13, at: 2000 });
  assert.equal(m.v, 1 / 0.75);
});

test('the speed counted on: as said, else 1 playing, 0 paused, a wind as measured or not yet', () => {
  assert.equal(speedOf(16, true, true, 8), 16);
  assert.equal(speedOf(undefined, false, true, null), 1);
  assert.equal(speedOf(undefined, false, false, 8), 0);
  assert.equal(speedOf(undefined, true, true, 8), 8);
  assert.equal(speedOf(undefined, true, true, null), undefined);
  assert.equal(speedOf(Number.NaN, true, true, -4), -4);
});

test('a host winding at 8×, reporting every 250 ms, late by 20–180 ms: followed closely, no stand, no jump', () => {
  const SERVER_EPOCH = 1_793_000_000_000; // the server's clock (epoch ms); the page's starts at 0
  const travel = [20, 180, 40, 120, 60, 150, 30, 90, 170, 50, 110, 25];
  const truth = (tPage: number) => 30 + (8 * (tPage - 1000)) / 1000;
  const reports = travel.map((late, i) => {
    const so = 1000 + i * 250; // when the host's place was so, on the page's clock
    return { now: so + late, at: so + SERVER_EPOCH, pos: truth(so) };
  });
  const gaps: number[] = [];
  let measured: Measured | null = null;
  let course: Course | null = null;
  let ease: Ease | null = null;
  let shownBefore: number | null = null;
  let worst = 0;
  let biggestStep = 0;
  let next = 0;
  // From the third report on (the second gives the first speed), each 16 ms frame.
  for (let t = reports[1].now; t <= reports[reports.length - 1].now; t += 16) {
    while (next < reports.length && reports[next].now <= t) {
      const r = reports[next++];
      measured = measure(measured, { index: 0, pos: r.pos, at: r.at });
      const v = speedOf(undefined, true, true, measured.v);
      if (v === undefined) continue;
      const before = course ? shownAt(course, ease, t) : null;
      course = { pos: r.pos, at: readAt(gaps, r.at, r.now), v, a: 0 };
      ease = easeInto(before, course, t, true);
    }
    if (!course) continue;
    const shown = shownAt(course, ease, t);
    if (next > 2) worst = Math.max(worst, Math.abs(shown - truth(t)));
    if (shownBefore !== null) biggestStep = Math.max(biggestStep, Math.abs(shown - shownBefore));
    shownBefore = shown;
  }
  // The clock is read against the quickest report: within its lateness of the truth, at 8× (20 ms → 0.16 s).
  assert.ok(worst < 1.6, `shown at most ${worst.toFixed(2)} s from the truth`);
  // A 16 ms frame at 8× moves 0.128 s; a correction fading in adds a little, never a jump.
  assert.ok(biggestStep < 0.6, `largest step between frames ${biggestStep.toFixed(3)} s`);
});
