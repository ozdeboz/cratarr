// An account's room in words (AL04.1, lib/room): what counts against it of its GB, at or over it exactly when the server says
// full; none for an administrator. Out of reach (AL04.5) no room line and no floor, and a Want says it needs Cratarr.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { floorRow, gbOfRoom, lineOfRoom, roomLine } = await import('../src/lib/room.ts');
const { wantFailed } = await import('../src/lib/wantPath.ts');

/** The line's two numbers. */
function numbers(line: string | null): [number, number] {
  const m = line?.match(/^(\S+) of (\S+) GB$/);
  assert.ok(m, `a room line: ${line}`);
  return [Number(m[1]), Number(m[2])];
}

test('the room line: what counts of the room in GB, over it as it is, cut down and never rounded up (AL04.1)', () => {
  assert.equal(roomLine(40e9, 100), '40 of 100 GB');
  assert.equal(roomLine(130e9, 100), '130 of 100 GB');
  assert.equal(roomLine(99.6e9, 100), '99 of 100 GB');
  assert.equal(roomLine(100e9, 100), '100 of 100 GB');
  assert.equal(roomLine(0, 100), '0 of 100 GB');
  // A room set with decimals (OQ-R5) reads with its own.
  assert.equal(roomLine(399_999, 0.0004), '0.0003 of 0.0004 GB');
  assert.equal(roomLine(150.56e9, 150.5), '150.5 of 150.5 GB');
  // As the server says a room, in bytes, and as its sheet starts.
  assert.equal(lineOfRoom({ size: 100e9, used: 130e9, full: true }), '130 of 100 GB');
  assert.equal(gbOfRoom({ size: 100e9, used: 0, full: false }), '100');
  // No room (an administrator): no line.
  assert.equal(roomLine(40e9, null), null);
  assert.equal(lineOfRoom(null), null);
});

test('the line reads at or over the room exactly when the server says full, for every used near it (AL04.1)', () => {
  for (const [gb, size] of [
    [100, 100e9],
    [0.0004, 400_000],
  ]) {
    const near = [size - 1, size, size + 1];
    for (let n = -200; n <= 200; n++) near.push(Math.max(0, Math.round(size + (n * size) / 10_000)));
    for (const used of near) {
      // The server's rule (`room_of`): full when used ≥ its size.
      const full = used >= size;
      const [shown, room] = numbers(roomLine(used, gb));
      assert.equal(room, gb, `${used} of ${gb}`);
      // Never "100 of 100 GB" while Wants still go ahead, nor a number under the room while they wait.
      assert.equal(shown >= room, full, `${used} bytes of ${gb} GB: ${roomLine(used, gb)}`);
    }
  }
});

test('out of reach: no room line and no floor, and a Want says it needs Cratarr, as today (AL04.5)', () => {
  assert.equal(roomLine(null, null), null);
  const room = { size: 100e9, used: 40e9, full: false };
  assert.equal(lineOfRoom(room, true), null, 'as last seen it may be over by now');
  assert.equal(lineOfRoom(room, false), '40 of 100 GB');
  const floor = { gb: 200, free: 150e9, short: true };
  assert.equal(floorRow(floor, true), null);
  assert.equal(floorRow(null), null);
  assert.equal(wantFailed(null, true), 'Wanting needs Cratarr.');
});
