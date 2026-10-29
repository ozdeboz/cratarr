// The app's deck as the page draws it (lib/deckCourse; the owner, 2026-10-29: a held wind's picture went in strides):
// reports at a wind's speed with their travel time jittering, read by when the deck was read; each correction fading
// in, never a jump; a seek or another track taken at once; an app without `deckAtMs` counted from the report's coming.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { courseAt, shownAt, easeInto, easing, readAt, EASE_MS } = await import('../src/lib/deckCourse.ts');
type Course = import('../src/lib/deckCourse.ts').Course;
type Ease = import('../src/lib/deckCourse.ts').Ease;

/** A wind at `v`× read every 100 ms on the app's clock, each report reaching the page after 5 to 25 ms: the place shown
 *  each 16 ms frame, from the first report's coming to the last's. */
function wind(v: number, withReadTime: boolean) {
  const APP_EPOCH = 7_000_000; // the app's clock, ms since its boot; the page's starts at 0
  const travel = [5, 25, 8, 19, 12, 25, 5, 16, 22, 9, 14, 25, 6, 18, 11];
  const reports = travel.map((late, i) => {
    const readPage = 1000 + i * 100;
    return { now: readPage + late, deckAtMs: readPage + APP_EPOCH, pos: 60 + (v * i * 100) / 1000 };
  });
  const gaps: number[] = [];
  let course: Course | null = null;
  let ease: Ease | null = null;
  const frames: { t: number; shown: number; truth: number }[] = [];
  let next = 0;
  for (let t = reports[0].now; t <= reports[reports.length - 1].now; t += 16) {
    while (next < reports.length && reports[next].now <= t) {
      const r = reports[next++];
      const before = course ? shownAt(course, ease, r.now) : null;
      course = { pos: r.pos, at: readAt(gaps, withReadTime ? r.deckAtMs : undefined, r.now), v, a: 0 };
      ease = easeInto(before, course, r.now, true);
    }
    frames.push({ t, shown: shownAt(course!, ease, t), truth: 60 + (v * (t - 1000)) / 1000 });
  }
  return frames;
}

/** The largest step from one frame to the next, as a share of the step the wind's speed makes in a frame. */
function worstStep(frames: { shown: number }[], v: number): number {
  let worst = 0;
  for (let i = 1; i < frames.length; i++) worst = Math.max(worst, Math.abs(frames[i].shown - frames[i - 1].shown) / ((v * 16) / 1000));
  return worst;
}

test('read by when the deck was read, a wind at 128× is drawn on its true place, whatever each report\'s travel', () => {
  const frames = wind(128, true);
  // The quickest travel seen sets the offset, so the place runs that far behind, the same every frame.
  const late = frames.slice(20).map((f) => f.shown - f.truth);
  assert.ok(Math.max(...late) - Math.min(...late) < 0.05, `the place wobbles by ${(Math.max(...late) - Math.min(...late)).toFixed(3)} s`);
  assert.ok(worstStep(frames, 128) < 1.2, `a frame's step reaches ${worstStep(frames, 128).toFixed(2)}× the speed's`);
});

test('without the read time, each report\'s travel is a correction faded in, never a jump', () => {
  const frames = wind(128, false);
  // Jumped to, a report 20 ms quicker than the one before puts the place 2.56 s on at once, on top of a frame's 2.05 s:
  // a step 2.25× the speed's. Faded in, it spreads over the frames of EASE_MS.
  assert.ok(worstStep(frames, 128) < 1.6, `a frame's step reaches ${worstStep(frames, 128).toFixed(2)}× the speed's`);
});

test('a correction is gone after EASE_MS, and fades in from where the place was shown', () => {
  const c: Course = { pos: 10, at: 0, v: 4, a: 0 };
  const e = easeInto(10.3, c, 0, true)!;
  assert.ok(Math.abs(e.gap - 0.3) < 1e-9);
  assert.ok(Math.abs(shownAt(c, e, 0) - 10.3) < 1e-9);
  assert.ok(easing(e, EASE_MS - 1));
  assert.ok(!easing(e, EASE_MS));
  assert.equal(shownAt(c, e, EASE_MS), courseAt(c, EASE_MS));
  const mid = shownAt(c, e, EASE_MS / 2) - courseAt(c, EASE_MS / 2);
  assert.ok(Math.abs(mid - 0.15) < 1e-9, 'half of it left half way');
});

test('a seek, a stall or another track is taken at once', () => {
  const c: Course = { pos: 100, at: 0, v: 4, a: 0 };
  // At 4× a gap past 2 s is not a report's travel.
  assert.equal(easeInto(97, c, 0, true), null);
  assert.notEqual(easeInto(98.5, c, 0, true), null);
  // At 128× up to 64 s of the course is still a correction.
  assert.notEqual(easeInto(140, { ...c, v: 128 }, 0, true), null);
  assert.equal(easeInto(99, c, 0, false), null);
  assert.equal(easeInto(null, c, 0, true), null);
});

test('braking to a stand, the course stands there and does not come back', () => {
  const c: Course = { pos: 50, at: 0, v: 20, a: -40 };
  assert.equal(courseAt(c, 500), 55);
  assert.equal(courseAt(c, 2000), 55);
});
