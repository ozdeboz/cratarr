// Golden frames (0.10, the architecture audit's visuals step 1): every rack the references cover (the standard
// rack, the library's examples, Fire among them) draws today what it drew when they were made: what the frame loop
// hands the renderer, frame by frame over a recorded track (harness/golden.ts). A change of the picture that is meant
// makes them again (harness/golden-make.ts); any other fails here.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { RACKS, departures, goldenFrames, readReference } = await import('../harness/golden.ts');

/** Rounding to 7 digits, and an engine's sums done in another order. */
const CLOSE = 1e-6;

test('the harness: the same rack twice is the same, frame for frame, and the music moves it', () => {
  for (const r of [RACKS[0], RACKS.find((x) => x.id.endsWith('fire'))!, RACKS[1]]) {
    const a = goldenFrames(r.patch, r.run);
    const b = goldenFrames(r.patch, r.run);
    const { worst, missing, extra } = departures(a, b);
    assert.equal(worst.d, 0, `${r.id}: ${worst.key} at frame ${worst.frame}`);
    assert.equal(missing.size + extra.size, 0);
    // The music in it: the Cratarr device's many numbers; an example's own clocks run in the compositor, its knobs and
    // fields here.
    const moving = [...a[0].keys()].filter((k) => a.some((f) => f.get(k) !== a[0].get(k)));
    assert.ok(moving.length >= (r.id === 'standard' || r.id.endsWith('fire') ? 50 : 1), `${r.id}: ${moving.length} numbers move`);
  }
});

for (const r of RACKS) {
  test(`${r.id} draws what its reference holds`, () => {
    const ref = readReference(r.id);
    assert.ok(ref, `${r.id}: no reference (make it with harness/golden-make.ts)`);
    const { frames, worst, missing, extra } = departures(ref, goldenFrames(r.patch, r.run));
    assert.equal(frames, null, `${r.id}: ${frames?.[1]} frames against ${frames?.[0]}`);
    assert.equal(missing.size, 0, `${r.id}: no longer gives ${[...missing].slice(0, 5).join(', ')}`);
    assert.equal(extra.size, 0, `${r.id}: gives ${[...extra].slice(0, 5).join(', ')} besides`);
    assert.ok(worst.d <= CLOSE, `${r.id}: ${worst.key} at frame ${worst.frame} is ${worst.got}, was ${worst.want}`);
  });
}
