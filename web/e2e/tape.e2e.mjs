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
      const ok = tape.start('t', fromMs, { kind: 'steady', v }, 1, (why) => (window.endedWhy = why));
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
    return { error, ready: tape.tape.ready, length: tape.tape.lengthMs };
  });
  assert.equal(r.error, null);
  assert.equal(r.ready, 't');
  assert.ok(Math.abs(r.length - 30000) < 1);
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
    tape.start('t', 29_700, { kind: 'steady', v: 1 }, 1, (w) => (window.endedWhy = w));
    for (let i = 0; i < 80 && !window.endedWhy; i++) await new Promise((r) => setTimeout(r, 25));
    return window.endedWhy;
  });
  assert.equal(why, 'end');
});
