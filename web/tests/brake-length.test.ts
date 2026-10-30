// A held wind's brake (startStop's brakeLengthMs; the owner, 2026-10-29: "When spinning down from high speeds, braking
// takes longer", constant deceleration): as long as its speed asks at DECEL_X × a second, the Brake as set its
// shortest. The same numbers as the app's Plan.lengthMs (0.9.15-2).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { brakeLengthMs, DECEL_X } = await import('../src/lib/startStop.ts');

test('from a high speed the brake takes longer, at one deceleration', () => {
  assert.equal(DECEL_X, 64);
  assert.equal(brakeLengthMs(128, 1, 500), 1984, '128× down to 1×: about 2 s');
  assert.equal(brakeLengthMs(64, 1, 500), 984, '64×: about 1 s');
  assert.equal(brakeLengthMs(-128, 0, 500), 2000, 'backwards, to a stand: its speed whatever its sign');
});

test('a slow wind keeps the Brake as set', () => {
  assert.equal(brakeLengthMs(32, 1, 500), 500);
  assert.equal(brakeLengthMs(16, 1, 500), 500);
  assert.equal(brakeLengthMs(8, 0, 1200), 1200);
});
