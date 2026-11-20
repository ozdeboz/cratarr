// Two administrators' switches in Connections (W13, lib/serverSwitches): Bandcamp's row says it is off, or until when its browser
// check holds it; "Pause background reading" is on while the server's background reading is off.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { backgroundPaused, bandcampHeld, bandcampRow, pausedBody } = await import('../src/lib/serverSwitches.ts');

const at = (t: number) => `t${t}`;

test("Bandcamp's row: off, or until when its check holds it, else nothing to say", () => {
  assert.deepEqual(bandcampRow({ enabled: false, held_until: 500 }, 100, at), { lead: 'off', line: 'Off' });
  assert.deepEqual(bandcampRow({ enabled: true, held_until: 500 }, 100, at), { lead: 'warn', line: 'Held until t500' });
  assert.deepEqual(bandcampRow({ enabled: true, held_until: 50 }, 100, at), { lead: 'on', line: null }, 'its day over');
  assert.deepEqual(bandcampRow({ enabled: true, held_until: null }, 100, at), { lead: 'on', line: null });
  assert.equal(bandcampHeld({ enabled: false, held_until: 500 }, 100), false, 'off is no hold');
});

test('Pause background reading is on while the reading is off, and sends the reading\'s own switch', () => {
  assert.equal(backgroundPaused({ enabled: false }), true);
  assert.equal(backgroundPaused({ enabled: true }), false);
  assert.deepEqual(pausedBody(true), { enabled: false });
  assert.deepEqual(pausedBody(false), { enabled: true });
});
