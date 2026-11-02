// The list filter's window lit on its profile's bars (lib/profileWindow; the owner, 2026-11-02): through the library's
// percentiles for energy and danceability, in BPM and years for tempo and year.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { insideOf, percentIn, windowsOf } = await import('../src/lib/profileWindow.ts');

const range = (axis: string, min: number | null, max: number | null, not = false) => ({ axis, not, value: { kind: 'range' as const, min, max } });
const row = (key: 'tempo' | 'energy', lo: number, hi: number, percentiles?: number[]) => ({ key, lo, hi, bins: [], q1: 0, mid: 0, q3: 0, percentiles });

test('a value among the hundredths, between two in proportion; past the ends at 0 and 100', () => {
  const table = Array.from({ length: 101 }, (_, i) => i / 1000);
  assert.ok(Math.abs(percentIn(table, 0.0425) - 42.5) < 1e-9);
  assert.equal(percentIn(table, -1), 0);
  assert.equal(percentIn(table, 1), 100);
});

test("the filter's ranges on each measure taken together; BPM is the tempo's; a left-out range is none", () => {
  const w = windowsOf([range('bpm', 100, null), range('bpm', null, 140), range('energy', 0.2, 0.6, true), { axis: 'genre', not: false, value: { kind: 'any_of', values: ['jazz'] } }]);
  assert.deepEqual(w, { tempo: [100, 140] });
  assert.deepEqual(windowsOf([range('bpm', 90, 150), range('bpm', 110, 130)]).tempo, [110, 130], 'each narrows it');
  assert.deepEqual(windowsOf(undefined), {});
});

test("on the bar: energy through the library's percentiles, tempo on its own scale, an open end at the bar's end", () => {
  // A library whose energy is squeezed low (0 to 0.1): 0.05 is its middle, half way along the bar.
  const energy = row('energy', 0, 100, Array.from({ length: 101 }, (_, i) => i / 1000));
  assert.deepEqual(insideOf(energy, [0.05, null]), [0.5, 1]);
  assert.deepEqual(insideOf(row('tempo', 60, 180), [90, 120]), [0.25, 0.5]);
  assert.equal(insideOf(row('tempo', 60, 180), undefined), null);
});
