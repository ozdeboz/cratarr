// The page's tape head in a browser (decision 0102, as amended; lib/tape): a track's file opened by its two decoding
// workers, read by the audio worklet forwards, backwards and above the top, where it is heard following, its end said.
// The track is a WAV made in the page (lib/tape/decoder reads WAVs until decode.wasm comes); no server needed.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, PAGE } from './browser.mjs';

let d;
before(async () => {
  d = await device(['--autoplay-policy=no-user-gesture-required']);
  await d.page.goto(`${PAGE}/e2e/tape-head.html`);
  await d.page.waitForFunction(() => window.ready === true, null, { timeout: 30_000 });
});
after(async () => {
  assert.deepEqual(d.errors, [], 'the page threw nothing');
  await d.browser.close();
});

/** Starts a reading and, `ms` later, where it is heard, how fast, and how loud. */
const reading = (fromMs, v, ms) =>
  d.page.evaluate(
    async ({ fromMs, v, ms }) => {
      const { tape } = window.t;
      window.endedWhy = null;
      const ok = tape.start('t', fromMs, { kind: 'steady', v }, 1, (why) => (window.endedWhy = why), () => (window.doneHeard = true));
      await new Promise((r) => setTimeout(r, ms));
      return { ok, heard: tape.heardMs(), speed: tape.heardSpeed(), loud: window.t.loudness() };
    },
    { fromMs, v, ms },
  );
const stopped = () =>
  d.page.evaluate(async () => {
    window.t.tape.stop();
    for (let i = 0; i < 40 && !window.endedWhy; i++) await new Promise((r) => setTimeout(r, 25));
    return window.endedWhy;
  });

test('a track’s file opened whole by its workers', async () => {
  const r = await d.page.evaluate(async () => {
    const { tape, wavUrl } = window.t;
    const error = await tape.load('t', wavUrl(30, 44100));
    // The loudness meter on the head's output from now on (it hears only what comes after it is there).
    window.t.loudness();
    return { error, ready: tape.tape.ready, length: tape.tape.lengthMs, by: tape.tape.by };
  });
  assert.equal(r.error, null);
  assert.equal(r.ready, 't');
  assert.ok(Math.abs(r.length - 30000) < 1);
  assert.equal(r.by, 'decode.wasm', 'opened by the decoder the app has too');
});

test('a hold let go brakes to a stand, the engine saying where', async () => {
  const r = await d.page.evaluate(async () => {
    const { tape } = window.t;
    window.endedWhy = null;
    tape.start('t', 1000, { kind: 'hold', base: 2, top: 16, doubling: 500, sign: 1 }, 1, (why) => (window.endedWhy = why), () => (window.doneHeard = true));
    await new Promise((r) => setTimeout(r, 600));
    const before = tape.heardSpeed();
    tape.letGo(0, 300);
    await new Promise((r) => setTimeout(r, 2500));
    return { before, after: tape.heardSpeed(), standsAt: tape.tape.standsAt, heard: tape.heardMs() };
  });
  assert.ok(r.before >= 2, `holding: ${r.before}`);
  assert.ok(Math.abs(r.after) < 0.01, `let go: ${r.after}`);
  assert.ok(r.standsAt !== null && r.standsAt > 1000, `stands at ${r.standsAt}`);
  assert.ok(Math.abs(r.heard - r.standsAt) < 5, `heard at ${r.heard}, standing at ${r.standsAt}`);
  assert.equal(await stopped(), 'stopped');
});

test('forwards: heard from where it began, at its pace, and heard at all', async () => {
  const r = await reading(2000, 1, 800);
  assert.ok(r.ok);
  assert.ok(r.heard > 2000 && r.heard < 2900, `heard at ${r.heard}`);
  assert.equal(r.speed, 1);
  assert.ok(r.loud > 0.05, `loudness ${r.loud}`);
  assert.equal(await stopped(), 'stopped');
});

test('backwards: heard going back', async () => {
  const r = await reading(6000, -1, 800);
  assert.ok(r.heard < 6000 && r.heard > 5100, `heard at ${r.heard}`);
  assert.equal(r.speed, -1);
  assert.ok(r.loud > 0.05, `loudness ${r.loud}`);
  assert.equal(await stopped(), 'stopped');
});

test('above the top: far along, in pieces', async () => {
  const r = await reading(1000, 32, 800);
  assert.ok(r.heard > 8000, `heard at ${r.heard}`);
  assert.equal(r.speed, 32);
  assert.equal(await stopped(), 'stopped');
});

test('at a stand: silence', async () => {
  const r = await reading(4000, 0, 500);
  assert.ok(r.loud < 0.001, `loudness ${r.loud}`);
  await stopped();
});

test('past the track’s end it ends, and says so', async () => {
  const why = await d.page.evaluate(async () => {
    const { tape } = window.t;
    window.endedWhy = null;
    tape.start('t', 29_700, { kind: 'steady', v: 1 }, 1, (w) => (window.endedWhy = w), () => (window.doneHeard = true));
    for (let i = 0; i < 80 && !window.endedWhy; i++) await new Promise((r) => setTimeout(r, 25));
    return window.endedWhy;
  });
  assert.equal(why, 'end');
});

test('its gain followed while it reads, without starting again', async () => {
  const r = await d.page.evaluate(async () => {
    const { tape, loudness } = window.t;
    window.endedWhy = null;
    tape.start('t', 3000, { kind: 'steady', v: 1 }, 1, (w) => (window.endedWhy = w), () => (window.doneHeard = true));
    await new Promise((r) => setTimeout(r, 600));
    const full = loudness();
    tape.setGain(0.25);
    await new Promise((r) => setTimeout(r, 400));
    return { full, quarter: loudness() };
  });
  assert.ok(r.full > 0.05, `full: ${r.full}`);
  assert.ok(Math.abs(r.quarter / r.full - 0.25) < 0.05, `a quarter: ${r.quarter} of ${r.full}`);
  assert.equal(await stopped(), 'stopped');
});

test('a hold carried on from a track before starts where its motion had got', async () => {
  const r = await d.page.evaluate(async () => {
    const { tape } = window.t;
    const hold = { kind: 'hold', base: 2, top: 16, doubling: 500, sign: 1 };
    window.endedWhy = null;
    tape.start('t', 1000, hold, 1, (w) => (window.endedWhy = w), () => (window.doneHeard = true), { motionAt: 2000, letGo: null });
    await new Promise((r) => setTimeout(r, 400));
    return tape.heardSpeed();
  });
  // Two seconds into a hold doubling every half second from 2×: at its top.
  assert.equal(r, 16, `carried on: ${r}`);
  assert.equal(await stopped(), 'stopped');
});

test('a let-go says when it is heard done, standing where the engine foretold', async () => {
  const r = await d.page.evaluate(async () => {
    const { tape } = window.t;
    const hold = { kind: 'hold', base: 2, top: 16, doubling: 500, sign: 1 };
    window.endedWhy = null;
    window.doneHeard = false;
    tape.start('t', 1000, hold, 1, (w) => (window.endedWhy = w), () => (window.doneHeard = true));
    await new Promise((r) => setTimeout(r, 500));
    const made = tape.letGoMade();
    const heldDone = window.doneHeard;
    tape.letGo(0, 300);
    await new Promise((r) => setTimeout(r, 100));
    const lg = tape.letGoMade();
    const foretold = lg && tape.landingIf(hold, 0, 1000, lg.atMs, 0, 300);
    for (let i = 0; i < 100 && !window.doneHeard; i++) await new Promise((r) => setTimeout(r, 30));
    const done = window.doneHeard;
    await new Promise((r) => setTimeout(r, 200));
    return { made, heldDone, lg, foretold, standsAt: tape.tape.standsAt, done, heard: tape.heardMs() };
  });
  assert.equal(r.made, null, 'none before a let-go');
  assert.equal(r.heldDone, false, 'a hold is never done by itself');
  assert.ok(r.lg && r.lg.atMs > 0, `the let-go as the head took it: ${JSON.stringify(r.lg)}`);
  assert.ok(r.done, 'done');
  assert.ok(Math.abs(r.foretold.place - r.standsAt) < 1, `foretold ${r.foretold?.place}, standing ${r.standsAt}`);
  assert.ok(Math.abs(r.heard - r.standsAt) < 5, `heard at ${r.heard}`);
  assert.equal(await stopped(), 'stopped');
});

test('a spin-up from a stand is heard done at its speed, and a tap when it lands', async () => {
  const r = await d.page.evaluate(async () => {
    const { tape } = window.t;
    const wait = async (ms) => {
      const t0 = performance.now();
      for (let i = 0; i < 200 && !window.doneHeard; i++) await new Promise((r) => setTimeout(r, 20));
      return window.doneHeard ? performance.now() - t0 : null;
    };
    window.doneHeard = false;
    tape.start('t', 2000, { kind: 'rise', v: 1, attack: 400 }, 1, (w) => (window.endedWhy = w), () => (window.doneHeard = true));
    const rose = await wait();
    const riseSpeed = tape.heardSpeed();
    tape.stop();
    await new Promise((r) => setTimeout(r, 100));
    window.doneHeard = false;
    // 3 s of track over 1 s, landing still.
    tape.start('t', 2000, { kind: 'tap', way: 3000, length: 1000, attack: 200, release: 200, floor: 0, sign: 1 }, 1, (w) => (window.endedWhy = w), () => (window.doneHeard = true));
    const tapped = await wait();
    await new Promise((r) => setTimeout(r, 150));
    return { rose, riseSpeed, tapped, heard: tape.heardMs() };
  });
  assert.ok(r.rose !== null && r.rose >= 350, `the spin-up done after its attack: ${r.rose}`);
  assert.ok(Math.abs(r.riseSpeed - 1) < 0.01, `at 1x: ${r.riseSpeed}`);
  assert.ok(r.tapped !== null && r.tapped >= 900, `the tap done as it lands: ${r.tapped}`);
  assert.ok(Math.abs(r.heard - 5000) < 30, `landed 3 s on: ${r.heard}`);
  assert.equal(await stopped(), 'stopped');
});

test('Play from of a track the head holds opens the copy named, not the one it holds, and the plain track again after', async () => {
  const r = await d.page.evaluate(async () => {
    const { headKey, tape, wavUrl } = window.t;
    const track = { source: 'cratarr', id: 'tr-1' };
    const opened = async (t, seconds) => {
      const key = headKey(t);
      const error = await tape.load(key, wavUrl(seconds, 44100));
      return { error, held: tape.tape.ready === key, length: Math.round(tape.tape.lengthMs) };
    };
    return [await opened(track, 5), await opened({ ...track, from: 'cratarr//media/Music' }, 7), await opened(track, 9)];
  });
  assert.deepEqual(r, [
    { error: null, held: true, length: 5000 },
    { error: null, held: true, length: 7000 },
    { error: null, held: true, length: 9000 },
  ]);
});
