// The Clock's trigger outputs (engine/clock.ts, decision 0057 §4b) and the clocked LFO, over the recorded house track
// (tr-10329, with its beats): a beat for every beat passed, a bar for every downbeat, ÷2 every other beat, ×2 half
// way through each, and the LFO back at the cycle's start on every bar.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { FPS, track } from './harness/recorded.ts';

const { newClockTriggers, stepClockTriggers, RATIOS, UNIT_RATIO } = await import('../src/lib/visuals/engine/clock.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { ID, seriesAt } = await import('../src/lib/visuals/signals.ts');
const { MODULATORS, defaultsOf } = await import('../src/lib/visuals/modulators/modulators.ts');
type Music = import('../src/lib/visuals/music-core.ts').Music;

const HOUSE = track('tr-10329');
const FROM = 30;
const SECONDS = 90;
const dt = 1 / FPS;
const step = (r: number) => RATIOS.indexOf(r as (typeof RATIOS)[number]);

/** The track's frames from FROM, as the precheck steps them: each frame's position and Music (its beat copied). */
const frames: { pos: number; m: Music }[] = (() => {
  const state = newMusicState();
  const out: { pos: number; m: Music }[] = [];
  for (let k = 0; k < SECONDS * FPS; k++) {
    const pos = FROM + k * dt;
    const m = musicOf(HOUSE.signals, HOUSE.beats, pos, true, { state, at: pos * 1000 });
    out.push({ pos, m: { ...m, beat: m.beat && { ...m.beat }, track: m.track.slice() } });
  }
  return out;
})();

/** Each frame's triggers at these knobs. */
function run(knobs: Record<string, number> = {}) {
  const st = newClockTriggers();
  return frames.map(({ m }) => stepClockTriggers(st, m, knobs, dt));
}
const firing = (outs: { beat: number; bar: number; phrase: number }[], o: 'beat' | 'bar' | 'phrase') => outs.flatMap((x, f) => (x[o] ? [f] : []));

/** The frame each beat in the window is first passed at (none for one at or before the first frame). */
const beatFrames: { beat: number; frame: number }[] = (() => {
  const at = HOUSE.beats!.at;
  const out: { beat: number; frame: number }[] = [];
  for (let i = 0; i < at.length; i++) {
    if (!(at[i] > frames[0].pos) || at[i] > frames.at(-1)!.pos) continue;
    out.push({ beat: i, frame: frames.findIndex((x) => x.pos >= at[i]) });
  }
  return out;
})();
/** Whether beat `i` starts its bar, from the server's bar series itself. */
const startsBar = (i: number) => Math.round((seriesAt(HOUSE.signals, ID.bar, 0, i)?.[0] ?? 1) * 255) === 0;

test('a beat fires for every beat passed, on the frame it is passed', () => {
  const beats = firing(run(), 'beat');
  assert.ok(beatFrames.length > 150, `${beatFrames.length} beats`);
  assert.equal(beats.length, beatFrames.length);
  assert.deepEqual(beats, beatFrames.map((b) => b.frame));
});

test('a bar fires for every downbeat, and a phrase every four bars (eight at 8)', () => {
  const outs = run();
  const bars = firing(outs, 'bar');
  const downbeats = beatFrames.filter((b) => startsBar(b.beat)).map((b) => b.frame);
  assert.ok(downbeats.length > 30, `${downbeats.length} downbeats`);
  assert.deepEqual(bars, downbeats);
  const phrases = firing(outs, 'phrase');
  assert.deepEqual(phrases, bars.filter((_, i) => i % 4 === 0));
  assert.deepEqual(firing(run({ phraseBars: 8 }), 'phrase'), bars.filter((_, i) => i % 8 === 0));
});

test('÷2 fires every other beat (from the bar), ÷4 a beat a bar; ÷2 on bars every other bar', () => {
  const all = run();
  const beats = firing(all, 'beat');
  const bars = new Set(firing(all, 'bar'));
  const half = run({ beatRatio: step(1 / 2) });
  const fired = firing(half, 'beat');
  // Every one of them a beat, every other beat counted from each bar's first.
  assert.ok(fired.every((f) => beats.includes(f)));
  let since = -1;
  const want: number[] = [];
  for (const f of beats) {
    since = bars.has(f) ? 0 : since + 1;
    if (since >= 0 ? since % 2 === 0 : false) want.push(f);
  }
  const before = beats.filter((f) => f < Math.min(...bars)).filter((_, i) => i % 2 === 0);
  assert.deepEqual(fired, [...before, ...want]);
  assert.ok(Math.abs(fired.length - beats.length / 2) <= 2, `${fired.length} of ${beats.length}`);
  // On a track of four-beat bars, ÷4 on beats is the bars' first beats.
  assert.deepEqual(firing(run({ beatRatio: step(1 / 4) }), 'beat').filter((f) => f >= Math.min(...bars)), [...bars]);
  assert.deepEqual(firing(run({ barRatio: step(1 / 2) }), 'bar'), [...bars].filter((_, i) => i % 2 === 0));
});

test('×2 fires on each beat and once more half way through it, with no drift', () => {
  const beats = new Set(firing(run(), 'beat'));
  const twice = firing(run({ beatRatio: step(2) }), 'beat');
  const mid = twice.filter((f) => !beats.has(f));
  for (const f of beats) assert.ok(twice.includes(f));
  assert.ok(mid.length >= beats.size - 2, `${mid.length} between ${beats.size} beats`);
  for (const f of mid) {
    const b = frames[f].m.beat!;
    const into = b.since / b.length;
    // The first frame past half way: at or past it, by less than a frame.
    assert.ok(into >= 0.5 && into - dt / b.length < 0.5, `frame ${f}: ${into} of its beat`);
  }
  // ×4 on bars: a beat a bar, the bars' four-beat tracks giving every beat.
  const quarters = firing(run({ barRatio: step(4) }), 'bar');
  const first = Math.min(...firing(run(), 'bar'));
  assert.deepEqual(quarters, [...beats].filter((f) => f >= first));
  assert.equal(RATIOS[UNIT_RATIO], 1);
});

test("the clocked LFO: a cycle a clock tick times its ratio, back to the cycle's start on every bar", () => {
  const triggers = run();
  const bars = new Set(firing(triggers, 'bar'));
  const lfo = (knobs: Record<string, number>, reset: boolean) => {
    const m = MODULATORS.lfo.create();
    return triggers.map((x) => m.step(dt, () => 0, { ...defaultsOf(MODULATORS.lfo), amount: 1, ...knobs, clock: x.beat, ...(reset ? { reset: x.bar } : {}) }));
  };
  // A cycle every three beats, a sine: on a four-beat bar's downbeat it would be a third further each bar unless reset.
  const reset = lfo({ ratio: step(1 / 3) }, true);
  const free = lfo({ ratio: step(1 / 3) }, false);
  for (const f of bars) assert.ok(Math.abs(reset[f]) < 1e-12, `frame ${f}: ${reset[f]}`);
  assert.ok([...bars].some((f) => Math.abs(free[f]) > 0.5), 'without the reset the bars fall all over the cycle');
  // ×1: the cycle is the beat. A saw (2) is at its start (0) on each beat, and rises with the beat: twice the share
  // of it gone (it starts at 0 rising, as the sine does), within what a frame's clock can tell: a trigger lands on the
  // first frame after its beat, and the period it runs at is the last ticks', whole frames apart (a frame each, the
  // second for as much of the beat as has gone).
  const saw = lfo({ ratio: UNIT_RATIO, shape: 2 }, true);
  const beats = firing(triggers, 'beat');
  for (const f of beats.slice(2)) assert.ok(Math.abs(saw[f]) < 1e-12, `frame ${f}: ${saw[f]}`);
  let checked = 0;
  for (let f = beats[2]; f < frames.length; f++) {
    const b = frames[f].m.beat;
    if (!b || b.since / b.length > 0.4) continue;
    assert.ok(Math.abs(saw[f] - (2 * b.since) / b.length) < (2 * dt * (1 + b.since / b.length)) / b.length + 0.02, `frame ${f}: ${saw[f]} at ${b.since / b.length} of the beat`);
    checked++;
  }
  assert.ok(checked > 500, `${checked} frames`);
});

test('unclocked, the LFO is as it always was (reset or not), its four waves in turn', () => {
  const free = (extra: Record<string, number>) => {
    const m = MODULATORS.lfo.create();
    return Array.from({ length: 200 }, (_, i) => m.step(dt, () => 0, { rate: 0.7, shape: 0.4, offset: 0.1, amount: 1, ...extra, ...(i % 30 === 0 ? { reset: 1 } : {}) }));
  };
  const was = (i: number) => {
    let u = 0;
    for (let k = 0; k <= i; k++) u = (u + 0.7 * dt) % 1;
    const p = u + 0.1;
    const sine = Math.sin(2 * Math.PI * p);
    const tri = 4 * Math.abs((((p - 0.25) % 1) + 1) % 1 - 0.5) - 1;
    return sine + (tri - sine) * 0.4;
  };
  free({}).forEach((v, i) => assert.ok(Math.abs(v - was(i)) < 1e-9, `${i}: ${v} vs ${was(i)}`));
  // At a quarter cycle: the sine and triangle at the top, the saw half way up, the square high.
  const at = (shape: number) => MODULATORS.lfo.create().step(1, () => 0, { rate: 0.25, shape, offset: 0, amount: 1 });
  assert.deepEqual([0, 1, 2, 3].map((s) => Math.round(at(s) * 1e9) / 1e9), [1, 1, 0.5, 1]);
});
