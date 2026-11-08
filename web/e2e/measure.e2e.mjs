// The measuring page for 0102's gate (e2e/measure-engine.html) in a browser: each of its three ways plays its generated
// track, sound heard keeps up with the clock with no gap, and it says when the first sound came. No server needed. What
// only a device can say (the screen off, the lock screen, the battery, the microphone's delay) is for the measuring day.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, PAGE } from './browser.mjs';

let d;
before(async () => {
  d = await device(['--autoplay-policy=no-user-gesture-required']);
  await d.page.goto(`${PAGE}/e2e/measure-engine.html`);
  await d.page.waitForFunction(() => !!window.measure, null, { timeout: 30_000 });
});
after(async () => {
  assert.deepEqual(d.errors, [], 'the page threw nothing');
  await d.browser.close();
});

/** Plays `mode` for `ms`, stops, and gives what was kept. */
const measured = (mode, ms) =>
  d.page.evaluate(
    async ({ mode, ms }) => {
      document.querySelector(`[data-mode="${mode}"]`).click();
      await window.measure.start(true);
      await new Promise((r) => setTimeout(r, ms));
      window.measure.stop();
      return window.measure.log();
    },
    { mode, ms },
  );

for (const mode of ['engine', 'element', 'plain']) {
  test(`${mode}: sound keeps up with the clock, with no gap, and its first sound is told`, async () => {
    const log = await measured(mode, 4000);
    assert.equal(log.mode, mode);
    assert.equal(log.error, undefined, `no error: ${log.error}`);
    assert.ok(log.firstSoundMs !== null && log.firstSoundMs < 3000, `first sound: ${log.firstSoundMs} ms`);
    assert.ok(log.elapsedS > 1.5, `measured for ${log.elapsedS} s`);
    assert.ok(Math.abs(log.soundS - log.elapsedS) < 0.3, `sound ${log.soundS} s over ${log.elapsedS} s`);
    assert.deepEqual(log.gaps, []);
  });
}
