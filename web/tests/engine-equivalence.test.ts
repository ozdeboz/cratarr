// The one engine against today's (decision 0057, step 2), frame by frame over recorded tracks. Until the one engine
// runs, today's against itself: the recording is whole and the same every run, on every track.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, compare, record, todays, track } from './harness/aspects-record.ts';

const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const settings = structuredClone(DEFAULTS);

test('the recorded tracks decode, the groove with its beats', () => {
  const ts = TRACKS.map(track);
  assert.ok(ts.every((t) => t.signals.series.size > 5));
  assert.ok(ts[0].beats && ts[0].beats.at.length > 100);
});

test("today's engine, recorded twice, is the same frame for frame: the harness itself", () => {
  for (const id of TRACKS) {
    const t = track(id);
    const a = record(todays(), t, settings, 30, 10);
    const b = record(todays(), t, settings, 30, 10);
    assert.equal(a.length, 300);
    assert.ok(a[0].size > 100, `${a[0].size} numbers a frame`);
    for (const [k, d] of compare(a, b)) assert.equal(d.most, 0, `${id} ${k} at frame ${d.frame}`);
    // It moves: the music is in it.
    const moved = [...compare(a, a.map(() => a[0]))].filter(([, d]) => d.most > 1e-3).length;
    assert.ok(moved > 10, `${id}: ${moved} fields move`);
  }
});
