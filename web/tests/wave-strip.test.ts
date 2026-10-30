// A hold's waveform as one strip on one grid (lib/waveStrip; 0.12, the owner: "the waveform is no longer pure through
// zoom!!! Not the same number of sections in the viewport permanently"): n bars across whatever the zoom, each the
// music's envelope at its middle at its span, each track against its own loudest; where a strip stands as it slides and
// scales.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { loudness, gridCover, gridBars, trackBars, planStrip, placeOf, covers, barTracks, originFor, SPAN_SLACK } = await import('../src/lib/waveStrip.ts');

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
const tracks = [
  { key: 'a', start: -180, d: 180, bins: binsOf(1) },
  { key: 'b', start: 0, d: 240, bins: binsOf(2, 90) },
  { key: 'c', start: 240, d: 200, bins: binsOf(3, 200) },
];

test('each track against its own loudest: a quiet track reaches the top as a loud one does, whatever is beside it', () => {
  for (const t of tracks) assert.ok(Math.abs(loudness([t], t.start, t.start + t.d) - 1) < 1e-6, t.key);
  assert.equal(loudness(tracks, 1000, 1010), 0, 'none past the tracks');
  // One gain per track: a bar inside b is the same with its neighbours there or not.
  for (const [a, b] of [
    [20, 21],
    [100, 104],
    [30, 90],
  ])
    assert.equal(loudness(tracks, a, b), loudness([tracks[1]], a, b));
});

test('a strip drawn anew on the same grid has the same bars where both cover: each bar its place alone', () => {
  for (const span of [0.05, 0.37, 1.3, 4]) {
    const a = gridCover(span, 7.25, -30, 60);
    const b = gridCover(span, 7.25, 10, 130);
    const ba = gridBars(tracks, a);
    const bb = gridBars(tracks, b);
    let shared = 0;
    for (let g = Math.max(a.g0, b.g0); g < Math.min(a.g0 + a.count, b.g0 + b.count); g++) {
      assert.equal(ba[g - a.g0], bb[g - b.g0], `span ${span}, bar ${g}`);
      shared++;
    }
    assert.ok(shared > 10, `span ${span}: they overlap`);
  }
});

test("a bar's height moves on continuously as its span changes: no pop as the zoom goes (the envelope between window sizes)", () => {
  for (const mid of [12.3, 47.9, 101.4, 230.2]) {
    let last: number | null = null;
    let worst = 0;
    for (let span = 0.05; span < 60; span *= 1.002) {
      const h = loudness(tracks, mid - span / 2, mid + span / 2);
      if (last !== null) worst = Math.max(worst, Math.abs(h - last));
      last = h;
    }
    // A step of 0.2 % in span: the height a hair, never a jump.
    assert.ok(worst < 0.03, `at ${mid}: ${worst.toFixed(4)} in a 0.2 % step`);
  }
});

test("a bar's height moves on continuously as its middle moves: a loud moment comes in as an edge nears it, not at once", () => {
  for (const span of [0.3, 2, 9]) {
    let last: number | null = null;
    let worst = 0;
    const step = span / 200;
    for (let mid = 5; mid < 200; mid += step) {
      const h = loudness([tracks[1]], mid - span / 2, mid + span / 2);
      if (last !== null) worst = Math.max(worst, Math.abs(h - last));
      last = h;
    }
    // A two-hundredth of a bar on: never more than a few hundredths of the height.
    assert.ok(worst < 0.04, `span ${span}: ${worst.toFixed(4)}`);
  }
});

test('finer than a bin: between the two nearest bins at its middle', () => {
  const t = [tracks[1]];
  const per = tracks[1].d / 512;
  const x = 100 * per + per * 0.3;
  const v = loudness(t, x - 0.01, x + 0.01);
  const lo = Math.min(tracks[1].bins[99], tracks[1].bins[100]) / 90;
  const hi = Math.max(tracks[1].bins[99], tracks[1].bins[100]) / 90;
  assert.ok(v >= lo - 1e-6 && v <= hi + 1e-6);
});

test('n bars across at every width: a strip is drawn anew once the bars shown would drift a quarter bar from n', () => {
  const n = 130;
  let had = null as ReturnType<typeof planStrip> | null;
  let drawn = 0;
  for (let w = 20; w < 400; w *= 1.004) {
    const from = 50 - w / 2;
    const to = 50 + w / 2;
    const s = planStrip(had, tracks, from, to, n, null);
    if (s !== had) drawn++;
    had = s;
    const shown = (to - from) / s.span;
    assert.ok(Math.abs(shown - n) <= SPAN_SLACK + 1e-9, `${w.toFixed(1)}: ${shown.toFixed(3)} bars`);
    assert.ok(covers(s, from, to));
  }
  assert.ok(drawn > 100, 'the zoom redraws as it goes');
});

test('at one zoom the grid stands in the music: kept while it covers, drawn anew on the same grid when it leaves', () => {
  let had = planStrip(null, tracks, -300, -180, 300, null);
  let drawn = 0;
  for (let from = 0; from < 600; from += 0.5) {
    const s = planStrip(had, tracks, from - 300, from - 180, 300, null);
    if (s !== had) {
      drawn++;
      assert.equal(s.span, had.span);
      assert.equal(s.o, had.o);
    }
    assert.ok(covers(s, from - 300, from - 180));
    had = s;
  }
  // 600 s slid with a 120 s window: drawn anew each 120 s or so, not each frame.
  assert.ok(drawn > 2 && drawn < 12, `${drawn} drawn`);
});

test('the zoom scales the grid about the middle of what shows: the bar there stays, none rush past', () => {
  const had = { span: 0.5, o: 3.1 };
  const mid = 120.37;
  const o = originFor(had, 0.6, mid, null, 130);
  // The middle's place among the bars is the same before and after.
  const phase = (x: number, sp: number, org: number) => (x - org) / sp;
  assert.ok(Math.abs(phase(mid, 0.6, o) - phase(mid, had.span, had.o)) < 1e-9);
});

test("zooming back out to the track's own view lands on its grid exactly: the plain waveform's bars, no step at the hand-over", () => {
  const n = 130;
  const home = { from: 0, to: 240 };
  const t = [tracks[1]];
  let had = null as ReturnType<typeof planStrip> | null;
  // In from a narrow window off the grid's phase, out again to the whole track.
  for (let k = 0; k <= 400; k++) {
    const w = 20 * (240 / 20) ** (k / 400);
    const mid = 77 + (120 - 77) * (k / 400);
    const s = planStrip(had, t, mid - w / 2, mid + w / 2, n, home);
    had = s;
  }
  const s = planStrip(had, t, 0, 240, n, home);
  assert.equal(s.span, 240 / n);
  assert.ok(Math.abs(s.o - Math.round(s.o / s.span) * s.span) < 1e-9, 'its origin on a bar edge of the track');
  const strip = gridBars(t, s);
  const plain = trackBars(tracks[1].bins, n);
  for (let i = 0; i < n; i++) {
    const g = Math.round(i - (s.o - 0) / s.span) - s.g0;
    assert.ok(Math.abs(strip[g] - plain[i]) < 1e-9, `bar ${i}`);
  }
});

test('a track whose waveform comes in later is drawn then', () => {
  const late = tracks.map((t) => (t.key === 'c' ? { ...t, bins: null } : t));
  const first = planStrip(null, late, 150, 270, 300, null);
  const again = planStrip(first, late, 150.1, 270.1, 300, null);
  assert.equal(again, first, 'kept while nothing came');
  const came = planStrip(again, tracks, 150.2, 270.2, 300, null);
  assert.notEqual(came, again, 'drawn anew once it came');
  assert.equal(came.span, again.span);
});

test('the stretch shown fills the waveform, edge to edge, as it slides and scales', () => {
  const s = gridCover(0.4, 1.3, 40, 100);
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
  const s = gridCover(0.9, 0.2, -20, 20);
  const owners = barTracks(tracks, s);
  for (let i = 0; i < s.count; i++) {
    const mid = s.o + (s.g0 + i + 0.5) * s.span;
    const k = owners[i];
    if (k >= 0) assert.ok(mid >= tracks[k].start && mid < tracks[k].start + tracks[k].d);
  }
});
