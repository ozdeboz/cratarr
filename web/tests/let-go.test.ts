// A hold let go (lib/player/letGo, as lib/localPlayer's letGo and land run it): one brake, at the wind's volume, the
// track's own player taking it over where the wind is heard and landing where that brings it; a hold let go before its
// wind sounds plays on by its fallback, the late wind never sounding as well; let go twice, the one brake goes on.
// The owner, 2026-10-31: on long wind-downs the end was heard twice, the second time as loud as the music.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { brakeLengthMs, brakeTravel } = await import('../src/lib/startStop.ts');
const { brakeGain, curveSpeed, letGoPlans, releaseOf } = await import('../src/lib/player/letGo.ts');
type Place = { index: number; pos: number };

/** Two tracks of 300 s; the hold let go 10 s before the first one's end, from `v0`×. */
function setup(v0 = 128) {
  const plans = letGoPlans((i) => (i === 20 || i === 21 ? 300 : null));
  const ms = brakeLengthMs(v0, 1, 500);
  const start: Place = { index: 20, pos: 290 };
  const c = { v0, to: 1, b0: 0, ms, start, order: [20, 21], dir: 1 as const, D: brakeTravel(v0, ms, ms / 1000, 1), aim: plans.along([20, 21], 1, start, brakeTravel(v0, ms, ms / 1000, 1)), skip: 0 };
  return { plans, c, ms };
}
const STEP = 16;
/** The track's own player's fastest (lib/rates RATE_TOP), and the wind stream's slowest rate (RATE_FLOOR). */
const TOP = 16;
const FLOOR = 1 / 16;

/** The wind's stream braked as lib/localPlayer does it (its rate the plan's speed over the stream's 128×, at most 1×,
 *  at least its floor), its rate changes heard 100 ms late, and the page standing still for 376 ms a second in (the
 *  ThinkPad's log, 02:21:59): until the hand-over. The way it is heard, and where it is handed over. */
function windDown(t: ReturnType<typeof setup>) {
  const { plans, c } = t;
  let heard: Place = c.start;
  const asked: { at: number; rate: number }[] = [{ at: 0, rate: 1 }];
  const heardWay: number[] = [];
  for (let now = STEP; now < c.ms; now += STEP) {
    // The stream's rate as heard now: the last one asked 100 ms ago.
    const rate = [...asked].reverse().find((a) => a.at <= now - 100)?.rate ?? 1;
    heard = plans.along(c.order, c.dir, heard, (c.v0 * rate * STEP) / 1000);
    heardWay.push(plans.wayTo(c, heard)!);
    const stalled = now > 1068 && now < 1444;
    if (!stalled) asked.push({ at: now, rate: Math.max(FLOOR, Math.min(1, plans.planSpeed(c, now, heard) / c.v0)) });
    const over = plans.handOver(c, now, heard, TOP);
    if (over) return { now, heard, heardWay, over };
  }
  throw new Error('never within the track’s reach');
}

test('a let-go from top speed: the track takes the brake over where the wind is heard, never where the plan had got', () => {
  const t = setup();
  const { now, heard, heardWay, over } = windDown(t);
  // The wind ran ahead of the plan (the stall, then its slowest): handing over at the plan's place played it twice.
  const behind = t.plans.wayTo(t.c, heard)! - t.plans.wayTo(t.c, t.plans.planAt(t.c, now))!;
  assert.ok(behind > 2, `the wind ahead of the plan by ${behind.toFixed(1)} s of track`);
  assert.deepEqual(over.at, heard, 'handed over where the wind is heard');
  assert.ok(over.at.index === 21 || over.at.pos >= heard.pos, 'nothing the wind played is played again');
  assert.ok(Math.max(...heardWay) <= t.plans.wayTo(t.c, over.at)! + 1e-9);
  assert.ok(Math.abs(over.speed - curveSpeed(t.c, now, t.c.ms)) < 1e-9 && over.speed <= TOP, 'at the curve’s own speed');
  // Carried on from there: no catching up (its speed the curve's), and its aim shifted by what the wind was ahead.
  assert.ok(Math.abs(t.plans.planSpeed(over.plan, now, heard) - over.speed) < 1e-9);
  const shift = t.plans.wayTo(t.c, over.plan.aim)! - t.plans.wayTo(t.c, t.c.aim)!;
  assert.ok(Math.abs(shift - behind) < 1e-6, 'its landing moved on by as much as the wind was ahead');
});

test("a let-go from top speed: one brake, down to 1× at its end, landing at the hand-over's own landing, at the wind's volume throughout", () => {
  const t = setup();
  const { now, over } = windDown(t);
  const gainDb = -6;
  const dim = brakeGain(gainDb);
  assert.ok(Math.abs(dim - 0.501) < 0.001);
  // The track's own player from the hand-over: the plan's speed where it stands, 1× to TOP×, the wind's volume.
  let place = over.at;
  let last = t.plans.wayTo(over.plan, place)!;
  const gains: number[] = [];
  let speed = over.speed;
  for (let at = now; at - over.plan.b0 < over.plan.ms; at += STEP) {
    speed = Math.min(TOP, Math.max(1, t.plans.planSpeed(over.plan, at, place)));
    place = t.plans.along(over.plan.order, 1, place, (speed * STEP) / 1000);
    const way = t.plans.wayTo(over.plan, place)!;
    assert.ok(way >= last, 'the tape only goes on: no stretch twice');
    last = way;
    gains.push(dim);
  }
  assert.ok(speed < 1.05, `down to 1× at its end (${speed.toFixed(2)}×)`);
  const off = t.plans.wayTo(over.plan, place)! - t.plans.wayTo(over.plan, over.plan.aim)!;
  assert.ok(Math.abs(off) < 0.05, `landed at its landing (${(off * 1000).toFixed(0)} ms off)`);
  assert.ok(gains.length > 10 && gains.every((g) => g === dim), 'the wind’s volume until the brake has ended');
});

test('a late wind: let go before it sounds, its fallback at once, and the wind never sounding after it', () => {
  // lib/localPlayer: the hold's wind asked, its clip not playing yet; let go (windRelease → letGo).
  let rewinding: object | null = { clip: 'asked' };
  const w = rewinding;
  const sounded: string[] = [];
  const release = releaseOf({ sounding: false, braking: false, handing: false });
  assert.equal(release, 'fallback');
  // The fallback (notInTime → plain): the wind ended, the track plays on from where it stood.
  if (release === 'fallback') {
    rewinding = null;
    sounded.push('track');
  }
  // The clip's 'playing' arrives late: no longer the wind under way (mine()), so nothing of it.
  if (rewinding === w) sounded.push('wind');
  assert.deepEqual(sounded, ['track']);
});

test('let go twice: the one brake goes on, never a second one', () => {
  const t = setup();
  const plans: (typeof t.c)[] = [];
  let w = { sounding: true, braking: false, handing: false };
  for (let i = 0; i < 2; i++) {
    const r = releaseOf(w);
    if (r === 'brake') {
      plans.push(t.c);
      // letGo: braking at once; forwards to 1×, handed to the track's player.
      w = { ...w, braking: true, handing: true };
    }
  }
  assert.equal(plans.length, 1);
  assert.equal(releaseOf({ sounding: true, braking: true, handing: false }), 'nothing', 'braking (a condensed one, or backwards)');
  assert.equal(releaseOf({ sounding: true, braking: false, handing: true }), 'nothing', 'handed over');
});
