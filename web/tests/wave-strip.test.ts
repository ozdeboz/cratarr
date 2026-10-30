// A hold's waveform as strips (lib/waveStrip; 0.11.1, the owner: "the waveforms are entirely unstable … we need
// everything a whole lot more stable"): each bar a function of its place in the hold's frame alone, each track against
// its own loudest, two levels of a power-of-two pyramid shown by the zoom; where a strip stands as it slides and scales.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { loudness, levelCover, levelBars, levelsOf, mixOf, planStrips, placeOf, covers, barTracks, pitchOf } = await import('../src/lib/waveStrip.ts');

/** A track's bins: a music-like envelope, its loudest `top` (its own scale). */
function binsOf(seed: number, top = 255, n = 512): Uint8Array {
  let s = seed;
  const r = () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
  const b = new Uint8Array(n);
  let sec = 0.6;
  for (let j = 0; j < n; j++) {
    if (r() < 0.03) sec = 0.3 + r() * 0.7;
    b[j] = Math.round(top * sec * (0.5 + 0.5 * r()));
  }
  b[100] = top;
  return b;
}
const UNIT = 1 / 64;
const tracks = [
  { key: 'a', start: -180, d: 180, bins: binsOf(1) },
  { key: 'b', start: 0, d: 240, bins: binsOf(2, 90) },
  { key: 'c', start: 240, d: 200, bins: binsOf(3, 200) },
];

test('each track against its own loudest: a quiet track reaches the top as a loud one does', () => {
  for (const t of tracks) assert.equal(loudness([t], t.start, t.start + t.d), 1, t.key);
  assert.equal(loudness(tracks, 1000, 1010), 0, 'none past the tracks');
});

test('a strip drawn anew elsewhere has the same bars where both cover: each bar its place alone', () => {
  for (const L of [2, 4, 6, 9]) {
    const a = levelCover(L, UNIT, -30, 60);
    const b = levelCover(L, UNIT, 10, 130);
    const ba = levelBars(tracks, a);
    const bb = levelBars(tracks, b);
    let shared = 0;
    for (let g = Math.max(a.g0, b.g0); g < Math.min(a.g0 + a.count, b.g0 + b.count); g++) {
      assert.equal(ba[g - a.g0], bb[g - b.g0], `level ${L}, bar ${g}`);
      shared++;
    }
    assert.ok(shared > 10, `level ${L}: they overlap`);
  }
});

test("the pyramid: a coarser bar is the loudest of the two finer ones in it, so the zoom's blend never flickers", () => {
  // Where a bar is at least a bin (a 240 s track's 512 bins: about half a second).
  for (const L of [6, 7, 8, 9]) {
    const fine = levelCover(L, UNIT, -20, 80);
    const coarse = levelCover(L + 1, UNIT, -20, 80);
    const f = levelBars(tracks, fine);
    const c = levelBars(tracks, coarse);
    for (let i = 0; i < c.length; i++) {
      const g = coarse.g0 + i;
      const k = 2 * g - fine.g0;
      if (k < 0 || k + 1 >= f.length) continue;
      assert.ok(Math.abs(c[i] - Math.max(f[k], f[k + 1])) < 1e-6, `level ${L + 1}, bar ${g}`);
    }
  }
});

test('finer than a bin: between the two nearest bins at its middle, the same for the same place', () => {
  const t = [tracks[1]];
  const per = tracks[1].d / 512;
  const x = 100 * per + per * 0.3;
  assert.equal(loudness(t, x - 0.01, x + 0.01), loudness(t, x - 0.01, x + 0.01));
  const v = loudness(t, x - 0.01, x + 0.01);
  const lo = Math.min(tracks[1].bins[99], tracks[1].bins[100]) / 90;
  const hi = Math.max(tracks[1].bins[99], tracks[1].bins[100]) / 90;
  assert.ok(v >= lo - 1e-6 && v <= hi + 1e-6);
});

test("the zoom's two levels: about n pieces shown, the coarser's share continuous with the width, each alone near its end", () => {
  const n = 300;
  let last: { fine: number; t: number } | null = null;
  for (let w = 20; w < 400; w *= 1.003) {
    const l = levelsOf(w, n, UNIT);
    const shown = w / pitchOf(l.fine, UNIT);
    assert.ok(shown >= n - 1e-6 && shown < 2 * n + 1e-6, `${w}: ${shown} finer bars`);
    if (last && last.fine === l.fine) assert.ok(Math.abs(mixOf(l.t) - mixOf(last.t)) < 0.02, `${w}: the share moved at once`);
    // Across an octave the coarser becomes the finer, shown whole on either side.
    if (last && last.fine !== l.fine) assert.ok(mixOf(last.t) > 0.99 && mixOf(l.t) < 0.01, `${w}: handed on whole`);
    last = l;
  }
  assert.equal(mixOf(0), 0);
  assert.equal(mixOf(1), 1);
});

test('planned strips: kept while they cover the stretch, drawn anew on the same grid when it leaves them', () => {
  type S = ReturnType<typeof planStrips>['strips'][number];
  let had: S[] = [];
  let drawn = 0;
  for (let from = 0; from < 600; from += 0.5) {
    const plan = planStrips(had, tracks, from - 300, from - 180, 300, UNIT);
    for (const s of plan.strips) {
      if (!had.includes(s)) drawn++;
      assert.ok(covers(s, from - 300, from - 180));
    }
    had = plan.strips;
  }
  // 600 s slid with a 120 s window: drawn anew each 120 s or so, not each frame.
  assert.ok(drawn > 2 && drawn < 12, `${drawn} drawn`);
});

test('a track whose waveform comes in later is drawn then', () => {
  const late = tracks.map((t) => (t.key === 'c' ? { ...t, bins: null } : t));
  const first = planStrips([], late, 150, 270, 300, UNIT);
  const again = planStrips(first.strips, late, 151, 271, 300, UNIT);
  assert.deepEqual(again.strips, first.strips, 'kept while nothing came');
  const came = planStrips(again.strips, tracks, 152, 272, 300, UNIT);
  assert.ok(came.strips.every((s) => !again.strips.includes(s)), 'drawn anew once it came');
});

test('the stretch shown fills the waveform, edge to edge, as it slides and scales', () => {
  const s = levelCover(5, UNIT, 40, 100);
  const W = 1000;
  for (const [from, to] of [
    [40, 100],
    [55, 110],
    [20, 75],
  ]) {
    const { x, k } = placeOf(s, from, to, W);
    // The strip is drawn as wide as the waveform; a place p of it at ((p - e0) / (e1 - e0)) · W, scaled by k, slid by x.
    const at = (p: number) => k * ((p - s.e0) / (s.e1 - s.e0)) * W + x;
    assert.ok(Math.abs(at(from)) < 1e-6 && Math.abs(at(to) - W) < 1e-6, `${from}..${to}`);
  }
});

test("each bar's track is the one its middle is in, for its colour", () => {
  const s = levelCover(9, UNIT, -20, 20);
  const owners = barTracks(tracks, s);
  for (let i = 0; i < s.count; i++) {
    const mid = (s.g0 + i + 0.5) * s.pitch;
    const k = owners[i];
    if (k >= 0) assert.ok(mid >= tracks[k].start && mid < tracks[k].start + tracks[k].d);
  }
});
