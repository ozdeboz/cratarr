import assert from 'node:assert/strict';
import test from 'node:test';
import { DUCK, EDGE, FADE_MS, FIXED_AT, SHORT_S, SNAP_S, SNIPPET_S, duckGain, ease, pickSnippet, snapToBeat, snippetGain, snippetOf } from '../src/lib/snippet.ts';

/** 512 bins of a quiet track with a loud stretch from `a` to `b` (shares of the track). */
function bins(a: number, b: number, loud = 240, quiet = 40): Uint8Array {
  const out = new Uint8Array(512).fill(quiet);
  for (let i = Math.floor(a * 512); i < Math.floor(b * 512); i++) out[i] = loud;
  return out;
}

test('the liveliest 20 s is where the loud stretch is', () => {
  const s = pickSnippet(bins(0.5, 0.62), 240);
  assert.equal(Math.round(s.to - s.from), SNIPPET_S);
  // The loud stretch is 0.5..0.62 of 240 s (120..148.8 s): the window lies inside it.
  assert.ok(s.from >= 119 && s.to <= 150, `${s.from}..${s.to}`);
});

test('a loud intro or outro is left out (the first and last EDGE)', () => {
  const d = 300;
  const intro = pickSnippet(bins(0, 0.07, 255), d);
  assert.ok(intro.from >= d * EDGE - 1e-9, `${intro.from}`);
  const outro = pickSnippet(bins(0.93, 1, 255), d);
  assert.ok(outro.to <= d * (1 - EDGE) + 1e-9, `${outro.to}`);
});

test('the earliest of equally lively windows', () => {
  const flat = new Uint8Array(512).fill(100);
  const s = pickSnippet(flat, 200);
  assert.ok(Math.abs(s.from - 200 * EDGE) < 200 / 512 + 1e-9, `${s.from}`);
});

test('a track under SHORT_S is its own snippet', () => {
  assert.deepEqual(pickSnippet(bins(0.2, 0.4), SHORT_S - 1), { from: 0, to: SHORT_S - 1 });
  assert.deepEqual(snippetOf(bins(0.2, 0.4), 12, [1, 2, 3]), { from: 0, to: 12 });
});

test('no stored waveform: from FIXED_AT of the track, on no beat', () => {
  const s = pickSnippet(null, 200);
  assert.equal(s.from, 200 * FIXED_AT);
  assert.equal(s.to, 200 * FIXED_AT + SNIPPET_S);
  // Its beats are not gone by without a waveform.
  assert.deepEqual(snippetOf(null, 200, [200 * FIXED_AT + 0.4]), s);
  // A track just long enough: kept before its last EDGE.
  const short = pickSnippet(null, 40);
  assert.ok(short.to <= 40 * (1 - EDGE) + 1e-9 && short.to - short.from > 0, `${short.from}..${short.to}`);
});

test('no length: nothing', () => {
  assert.deepEqual(pickSnippet(bins(0, 1), 0), { from: 0, to: 0 });
});

test('the start moves to the nearest beat within SNAP_S', () => {
  assert.equal(snapToBeat(100.4, [98, 99.5, 100.1, 101.2], 300, 20), 100.1);
  // None near enough: as it is.
  assert.equal(snapToBeat(100, [100 + SNAP_S + 0.2, 90], 300, 20), 100);
  // No beats: as it is.
  assert.equal(snapToBeat(50, null, 300, 20), 50);
  assert.equal(snapToBeat(50, [], 300, 20), 50);
});

test('a beat that would run the snippet past the end is not taken', () => {
  // 280.6 is nearer, but 20 s from it would end past 300 s: the next nearest within SNAP_S.
  assert.equal(snapToBeat(280.5, [279.2, 280.6], 300, 20), 279.2);
});

test('the snippet keeps its length when its start moves to a beat', () => {
  const b = bins(0.5, 0.62);
  const plain = snippetOf(b, 240);
  const beats = [plain.from - 0.6, plain.from + 0.3, plain.from + 2.5];
  const s = snippetOf(b, 240, beats);
  assert.equal(s.from, plain.from + 0.3);
  assert.ok(Math.abs(s.to - s.from - (plain.to - plain.from)) < 1e-9);
});

test('the fades: in over FADE_MS, out over the last FADE_MS, eased', () => {
  const L = SNIPPET_S * 1000;
  assert.equal(snippetGain(0, L), 0);
  assert.equal(snippetGain(FADE_MS / 2, L), 0.5);
  assert.equal(snippetGain(FADE_MS, L), 1);
  assert.equal(snippetGain(L / 2, L), 1);
  assert.equal(snippetGain(L - FADE_MS / 2, L), 0.5);
  assert.equal(snippetGain(L, L), 0);
  // Never a jump: each 16 ms frame moves it little.
  for (let ms = 0; ms < L; ms += 16) assert.ok(Math.abs(snippetGain(ms + 16, L) - snippetGain(ms, L)) < 0.09);
  // A very short one: half in, half out.
  assert.equal(snippetGain(200, 400), 1);
});

test('the duck: 1 to DUCK over FADE_MS and back, from wherever it stands', () => {
  assert.equal(duckGain(0, true), 1);
  assert.ok(Math.abs(duckGain(FADE_MS, true) - DUCK) < 1e-12);
  assert.ok(Math.abs(duckGain(FADE_MS / 2, true) - (1 + DUCK) / 2) < 1e-12);
  assert.equal(duckGain(0, false), DUCK);
  assert.equal(duckGain(FADE_MS, false), 1);
  // Back up from halfway down: starts there, ends at 1.
  assert.equal(duckGain(0, false, 0.6), 0.6);
  assert.equal(duckGain(FADE_MS, false, 0.6), 1);
  assert.equal(ease(-1), 0);
  assert.equal(ease(2), 1);
});
