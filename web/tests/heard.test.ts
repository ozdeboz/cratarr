// How fast a sound is heard going (lib/player/heard): measured on its own clock, never the rate set on it (the owner,
// 2026-11-03: "we need to measure the tape truthfully. No guessing anywhere").
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { HeardPace, SPAN_MS } = await import('../src/lib/player/heard.ts');

test('its speed is what its clock moved over the last span, not before it has a span', () => {
  const p = new HeardPace();
  p.read(0, 10);
  assert.equal(p.speed(), null);
  p.read(60, 10.06);
  assert.equal(p.speed(), null);
  p.read(SPAN_MS, 10 + SPAN_MS / 1000);
  assert.ok(Math.abs((p.speed() ?? 0) - 1) < 1e-9);
});

test('a rate set lower is heard only as the clock slows: the brake is measured, not assumed', () => {
  // At 1× for a while, then a rate of a quarter applied 100 ms late (as a browser does): right after the set, still 1×.
  const p = new HeardPace();
  let at = 0;
  for (let now = 0; now <= 1000; now += 16) {
    const rate = now < 600 + 100 ? 1 : 0.25;
    if (now > 0) at += (16 / 1000) * rate;
    p.read(now, at);
    // Set at 600 ms, heard from 700: at 688 ms what it is heard going is still 1×.
    if (now === 688) assert.ok(Math.abs((p.speed() ?? 0) - 1) < 1e-6, 'still 1× before the rate is heard');
  }
  assert.ok(Math.abs((p.speed() ?? 0) - 0.25) < 1e-6);
});

test('a jump (a seek, a new stream) starts the measure over', () => {
  const p = new HeardPace();
  p.read(0, 0);
  p.read(200, 0.2);
  assert.ok(p.speed() !== null);
  p.read(216, 50);
  assert.equal(p.speed(), null);
  p.read(232, 49);
  assert.equal(p.speed(), null);
});
