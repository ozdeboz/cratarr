// A continuous hold, frame by frame: the real window (lib/waveView's stepWindow) and strip (lib/waveStrip's planStrip)
// as lib/SeekWave and lib/Waveform run them, through a queue, from the wind-up to the let go and back to the track's
// own view.
// 0.11.1, the owner: "there is a lot of glitching in the waveform on continuous hold … we need everything a whole lot
// more stable, window included". Measured on 0.11.0: a moment of music's bar changed 668 times in 35 s, the strip
// cross-faded 11 times, the window lurched 1.5 s in a frame at the let go.
// 0.12, the owner: "the waveform is no longer pure through zoom!!! Not the same number of sections in the viewport
// permanently", and "it glitches and flashes", "bars jump sideways", "colour or brightness flips". Measured on 0.11.1's
// trunk (two power-of-two levels blended): 79 to 214 bars shown for n = 130, two levels on 170 of 2,091 frames.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { stepWindow, markShows } = await import('../src/lib/waveView.ts');
const { planStrip, gridBars, placeOf, trackBars, STRIP_SPAN, SPAN_SLACK } = await import('../src/lib/waveStrip.ts');

const LENS = [212, 187, 245, 301, 176, 233, 198, 264];
const STARTS = LENS.map((_, i) => LENS.slice(0, i).reduce((a, b) => a + b, 0));
const TOTAL = LENS.reduce((a, b) => a + b, 0);
const BINS = LENS.map((_, i) => {
  let s = i + 1;
  const r = () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
  const b = new Uint8Array(512);
  let sec = 0.6;
  for (let j = 0; j < 512; j++) {
    if (r() < 0.02) sec = 0.3 + r() * 0.7;
    b[j] = Math.round(255 * sec * (0.55 + 0.45 * r()));
  }
  return b;
});
const FRAME = 1000 / 60;
const N = 130;
const W = 390;

type Strip = ReturnType<typeof planStrip>;
type Seen = {
  t: number;
  held: boolean;
  /** What shows, in s of the hold's frame (lib/SeekWave's holdShown), and the playhead there. */
  from: number;
  to: number;
  pos: number;
  /** The playing track's place in the frame (lib/Waveform's cur0, cur1) and the playhead's share of it (`progress`). */
  cur0: number;
  cur1: number;
  progress: number;
  strip: Strip;
  bars: number[];
  /** The strip drawn anew this frame. */
  redrawn: boolean;
  /** Back to the track's own view: the plain waveform, the strip gone (the window null). */
  plain: boolean;
  /** The playing track's place in the queue. */
  track: number;
  /** What shows, in s along the whole queue. */
  qfrom: number;
  qto: number;
};
/** A hold's way and the queue's walls (lib/SeekWave's queueEnds): `walls` the queue's ends are walls, the tape resting
 *  at them; `known` the end known while `grow` s have not passed (a radio or Autoplay queue growing), then the whole. */
type Opts = { dir?: 1 | -1; walls?: boolean; known?: number; grow?: number };

/**
 * A hold from `q0` s along the queue: 4x doubling every 2 s to 16x, held `heldS` s, then braked to 1x in a second and
 * on for `afterS` s. Each frame: the window in the hold's frame, the strip shown with its bars.
 */
function hold(q0: number, heldS: number, afterS: number, through = true, o: Opts = {}): Seen[] {
  const dir = o.dir ?? 1;
  let win: ReturnType<typeof stepWindow> = null;
  let base: number | null = null;
  let had: Strip | null = null;
  const bars = new Map<object, number[]>();
  let q = q0;
  let v = 1;
  let idx = -1;
  const out: Seen[] = [];
  for (let t = 0; t < heldS + afterS; t += FRAME / 1000) {
    const holding = t < heldS;
    v = holding ? Math.min(16, 4 * 2 ** (t / 2)) : Math.max(1, v - 15 * (FRAME / 1000));
    const end = o.grow !== undefined && t < o.grow ? (o.known ?? TOTAL) : TOTAL;
    q += dir * v * (FRAME / 1000);
    if (o.walls) q = Math.max(0, Math.min(end - 1e-6, q));
    let i = 0;
    while (i < LENS.length - 1 && q >= STARTS[i + 1]) i++;
    const moved = idx < 0 || idx === i ? 0 : STARTS[idx] - STARTS[i];
    idx = i;
    const pos = q - STARTS[i];
    const was = win;
    const ends = o.walls ? { lo: -STARTS[i], hi: end - STARTS[i] } : undefined;
    const aim = holding ? (o.walls ? Math.max(ends!.lo, Math.min(ends!.hi, pos + (dir * v * v) / 128)) : pos + (dir * v * v) / 128) : null;
    win = stepWindow(win, { len: LENS[i], pos, moved, holding, through, dir, speed: v, top: 16, aim, ends }, FRAME);
    if (!win) {
      if (was) out.push({ ...out.at(-1)!, t, plain: true });
      continue;
    }
    if (!was) base = i;
    // lib/SeekWave: the frame counts from the start of the track the hold began in (`fa`, from this track's start).
    const fa = STARTS[base!] - STARTS[i];
    const from = win.shown.from - fa;
    const to = win.shown.to - fa;
    const reach = ((STRIP_SPAN - 1) / 2) * (to - from);
    const tracks = LENS.map((d, k) => ({ key: `t${k}`, start: STARTS[k] - STARTS[base!], d, bins: BINS[k] })).filter(
      (x) => (through || x.key === `t${i}`) && x.start < to + reach && x.start + x.d > from - reach,
    );
    const cur0 = -fa;
    const cur1 = LENS[i] - fa;
    const s = planStrip(had, tracks, from, to, N, { from: cur0, to: cur1 });
    const redrawn = s !== had;
    had = s;
    if (!bars.has(s)) bars.set(s, gridBars(tracks, s));
    out.push({ t, held: holding, from, to, pos: pos - fa, cur0, cur1, progress: pos / LENS[i], strip: s, bars: bars.get(s)!, redrawn, plain: false, track: i, qfrom: from + STARTS[base!], qto: to + STARTS[base!] });
  }
  return out;
}

const run = hold(30, 30, 6);
const drawn = run.filter((f) => !f.plain);

test('the same number of bars across the waveform on every frame: the wind-up, the top speed through tracks, the let go', () => {
  // 0.11.1's trunk: 79 to 214 for 130.
  for (const f of drawn) {
    const shown = (f.to - f.from) / f.strip.span;
    assert.ok(Math.abs(shown - N) <= SPAN_SLACK + 1e-9, `${f.t.toFixed(2)} s: ${shown.toFixed(3)} bars across`);
  }
  assert.ok(drawn.length > 2000);
});

test('no frame without its bars: one strip, covering the waveform edge to edge, every bar drawn', () => {
  for (const f of drawn) {
    assert.ok(f.strip.e0 <= f.from && f.strip.e1 >= f.to, `${f.t.toFixed(2)} s: the strip does not cover what shows`);
    assert.equal(f.bars.length, f.strip.count);
    assert.ok(f.bars.every((b) => Number.isFinite(b) && b >= 0 && b <= 1), `${f.t.toFixed(2)} s: a bar not drawn`);
  }
});

test('at one zoom the bars stand in the music: the same place, the same bar, frame after frame', () => {
  const seen = new Map<string, number>();
  let checked = 0;
  for (const f of drawn)
    for (let i = 0; i < f.bars.length; i += 5) {
      const key = `${f.strip.span}:${f.strip.o}:${f.strip.g0 + i}`;
      const was = seen.get(key);
      if (was !== undefined) {
        assert.equal(f.bars[i], was, `${f.t.toFixed(2)} s: bar ${f.strip.g0 + i} changed`);
        checked++;
      }
      seen.set(key, f.bars[i]);
    }
  assert.ok(checked > 10000, `${checked} checked`);
});

/** The bars' heights at the waveform's n places (each place's bar), a frame's picture. */
function picture(f: Seen): number[] {
  return Array.from({ length: N }, (_, j) => {
    const x = f.from + ((j + 0.5) / N) * (f.to - f.from);
    const g = Math.floor((x - f.strip.o) / f.strip.span) - f.strip.g0;
    return f.bars[Math.min(f.bars.length - 1, Math.max(0, g))];
  });
}

test('through the zoom each bar changes a little each frame, by how far its music moved: no pop, no flash', () => {
  // A strip drawn anew for a new span: each bar against the same bar before (as many bars from the middle, the grid
  // scaled about it), by how far its middle moved in the music (in its own spans) and how much its span changed.
  let worst = 0;
  let checked = 0;
  for (let k = 1; k < drawn.length; k++) {
    const a = drawn[k - 1];
    const b = drawn[k];
    if (!b.redrawn || b.t - a.t > FRAME / 1000 + 1e-9) continue;
    const sa = a.strip.span;
    const sb = b.strip.span;
    const mid = (b.from + b.to) / 2;
    for (let i = 0; i < b.strip.count; i++) {
      const cb = b.strip.o + (b.strip.g0 + i + 0.5) * sb;
      if (cb < b.from || cb > b.to) continue;
      const ga = Math.floor((mid + ((cb - mid) / sb) * sa - a.strip.o) / sa);
      const ia = ga - a.strip.g0;
      if (ia < 0 || ia >= a.bars.length) continue;
      const ca = a.strip.o + (ga + 0.5) * sa;
      const moved = Math.abs(cb - ca) / sb;
      const d = Math.abs(b.bars[i] - a.bars[ia]);
      // The envelope's slope: a whole height over half a bar at most, and over an octave of span.
      const allowed = 0.01 + 2.5 * moved + 1.5 * Math.abs(Math.log2(sb / sa));
      worst = Math.max(worst, d);
      checked++;
      assert.ok(d <= allowed, `${b.t.toFixed(2)} s, bar ${i}: ${d.toFixed(3)} (moved ${moved.toFixed(3)} bars, span ${((sb / sa - 1) * 100).toFixed(2)} %)`);
    }
  }
  assert.ok(checked > 1000, `${checked} checked`);
});

test('the bars move exactly as the window: where the strip puts each, the music under it maps to the same pixel', () => {
  for (const f of drawn) {
    const { x, k } = placeOf(f.strip, f.from, f.to, W);
    for (let i = 0; i < f.strip.count; i += 17) {
      const p = f.strip.o + (f.strip.g0 + i) * f.strip.span;
      const drawnAt = k * ((p - f.strip.e0) / (f.strip.e1 - f.strip.e0)) * W + x;
      const truth = ((p - f.from) / (f.to - f.from)) * W;
      assert.ok(Math.abs(drawnAt - truth) < 1e-6, `${f.t.toFixed(2)} s: bar ${i} at ${drawnAt.toFixed(4)} px, its music at ${truth.toFixed(4)}`);
    }
  }
});

test('the playhead stands where its music is among the bars: the same frame, the same mapping', () => {
  for (const f of drawn) {
    // lib/Waveform's `done`: the playing track's share mapped through its place in the frame.
    const done = (f.cur0 + f.progress * (f.cur1 - f.cur0) - f.from) / (f.to - f.from);
    const truth = (f.pos - f.from) / (f.to - f.from);
    assert.ok(Math.abs(done - truth) * W < 1e-6, `${f.t.toFixed(2)} s: playhead ${((done - truth) * W).toFixed(4)} px off`);
  }
});

test("let go and back to the track's own view: the last strip is the plain waveform's bars, no step at the hand-over", () => {
  const last = drawn.at(-1)!;
  assert.ok(run.at(-1)!.plain, 'it came back to the plain waveform');
  const len = last.cur1 - last.cur0;
  assert.ok(Math.abs(last.strip.span - len / N) < 1e-9);
  const plain = trackBars(BINS[last.track], N);
  const strip = picture(last);
  for (let j = 0; j < N; j++) assert.ok(Math.abs(strip[j] - plain[j]) < 1e-6, `place ${j}: ${strip[j]} then ${plain[j]}`);
});

/** How much the window's motion changes from one frame to the next (s of the queue per frame, per frame). */
function jolts(frames: Seen[]): { t: number; d: number; step: number }[] {
  const f = frames.filter((x) => !x.plain);
  const out = [];
  for (let k = 2; k < f.length; k++) {
    const a = f[k].from - f[k - 1].from;
    const b = f[k - 1].from - f[k - 2].from;
    out.push({ t: f[k].t, d: Math.abs(a - b), step: a });
  }
  return out;
}

test('the window never lurches: from the hold to the let go its motion changes a little each frame', () => {
  // 0.11.0: 1.48 s of the queue in a frame at the let go.
  for (const j of jolts(run)) assert.ok(j.d <= 0.25, `${j.t.toFixed(2)} s: its motion changed by ${j.d.toFixed(3)} s in a frame`);
});

test('let go, it goes on as it was going: never standing still a frame while the tape moves', () => {
  const after = jolts(run).filter((j) => j.t > 29.9 && j.t < 31);
  for (const j of after) assert.ok(j.step > 0.05, `${j.t.toFixed(2)} s: it stood (${j.step.toFixed(3)} s)`);
});

test("leaving a track's start, the window gathers speed rather than going at once", () => {
  // From a track's start, at the top speed: the start is the left edge until the crosshair can stand in the middle.
  const f = hold(STARTS[1], 12, 0);
  const leaving = jolts(f).filter((j) => j.t > 1);
  for (const j of leaving) assert.ok(j.d <= 0.03, `${j.t.toFixed(2)} s: ${j.d.toFixed(3)} s a frame at once`);
  assert.ok(leaving.at(-1)!.step > 0.2, "then at the tape's speed");
});

// The queue's own ends (0.12.3, the owner: "during the continuous hold, the window shouldn't allow viewing empty
// waveforms"): before the first track's start and after the last one's end nothing shows, and reaching them, no jump.
const EPS = 1e-6;
test("a hold back from the first track's start never shows before the queue", () => {
  const f = hold(25, 10, 3, true, { dir: -1, walls: true }).filter((x) => !x.plain);
  assert.ok(f.length > 500);
  for (const x of f) assert.ok(x.qfrom >= -EPS, `${x.t.toFixed(2)} s: shows ${(-x.qfrom).toFixed(3)} s before the queue`);
});

test("a hold on into the last track's end never shows past it, and no jump at the end", () => {
  const f = hold(TOTAL - 150, 16, 4, true, { walls: true });
  const drawn = f.filter((x) => !x.plain);
  for (const x of drawn) assert.ok(x.qto <= TOTAL + EPS, `${x.t.toFixed(2)} s: shows ${(x.qto - TOTAL).toFixed(3)} s past the queue`);
  // Held at the end, its width changes a little each frame (no zoom jump on reaching it; the zoom in as the hold
  // begins is the hand-over's, eased, as the tests above hold it).
  const atEnd = drawn.filter((x) => x.held && x.t > 4.5);
  for (let k = 1; k < atEnd.length; k++) {
    const dw = Math.abs(atEnd[k].to - atEnd[k].from - (atEnd[k - 1].to - atEnd[k - 1].from));
    assert.ok(dw <= 0.5, `${atEnd[k].t.toFixed(2)} s: its width changed by ${dw.toFixed(3)} s in a frame`);
  }
  // At the top speed, reaching the end and resting on it: no lurch. (Before it, the width runs up with the speed and
  // stops at once at the top, as it is meant to; against a wall that shows in the one edge that moves.)
  for (const j of jolts(f).filter((x) => x.t > 4.5)) assert.ok(j.d <= 0.25, `${j.t.toFixed(2)} s: its motion changed by ${j.d.toFixed(3)} s in a frame`);
  // And it stands against the end: what shows reaches it.
  assert.ok(drawn.some((x) => Math.abs(x.qto - TOTAL) <= 1e-3), 'it rests on the end');
});

test('a queue growing under a hold: its end moves on, what shows eases after it, no jump', () => {
  const f = hold(STARTS[6], 20, 0, true, { walls: true, known: STARTS[7], grow: 12 });
  for (const x of f.filter((y) => !y.plain && y.t < 12)) assert.ok(x.qto <= STARTS[7] + EPS, `${x.t.toFixed(2)} s: past the known end`);
  // The end moving on lets the window come off it toward the middle, sprung from standing: up to half its width in
  // under a second, so a little more each frame than a hold's own motion (0.3 against 0.25), never a step.
  for (const j of jolts(f)) assert.ok(j.d <= 0.5, `${j.t.toFixed(2)} s: its motion changed by ${j.d.toFixed(3)} s in a frame`);
});

test('at the walls the same number of bars across the waveform', () => {
  for (const f of [hold(25, 10, 3, true, { dir: -1, walls: true }), hold(TOTAL - 150, 16, 4, true, { walls: true })]) {
    for (const x of f.filter((y) => !y.plain)) {
      const shown = (x.to - x.from) / x.strip.span;
      // The planner's quarter-bar slack (landing on the track's own grid as it is let go: a ten-thousandth over).
      assert.ok(Math.abs(shown - N) <= SPAN_SLACK + 1e-3, `${x.t.toFixed(2)} s: ${shown.toFixed(4)} bars across`);
    }
  }
});

// A boundary's marker (0.12.3, the owner: "when track boundaries are snapped to the waveform viewport edges, they
// should not be shown"): none at an edge, whole inside, fading between.
test('a boundary marker: none within a bar of an edge, whole two bars in, no step between', () => {
  const n = N;
  assert.equal(markShows(0, n), 0);
  assert.equal(markShows(1, n), 0);
  assert.equal(markShows(0.5 / n, n), 0);
  assert.equal(markShows(1 - 1 / n, n), 0);
  assert.equal(markShows(2 / n, n), 1);
  assert.equal(markShows(0.5, n), 1);
  assert.ok(markShows(1.5 / n, n) > 0 && markShows(1.5 / n, n) < 1);
  // Moving off a wall a fraction of a bar a frame: its opacity never steps by more than the move.
  let last = markShows(0, n);
  for (let x = 0; x <= 0.1; x += 0.05 / n) {
    const o = markShows(x, n);
    assert.ok(Math.abs(o - last) <= 0.05 + 1e-9, `at ${x.toFixed(4)}: ${last.toFixed(3)} to ${o.toFixed(3)}`);
    last = o;
  }
});

test("a boundary on a wall shows no marker: from a track's start going forward, its start on the left edge", () => {
  const f = hold(STARTS[1], 6, 0).filter((x) => !x.plain);
  // While the window rests against the start (the start at the left edge), the start's marker does not show.
  const resting = f.filter((x) => Math.abs(x.qfrom - STARTS[1]) <= 1e-6);
  assert.ok(resting.length > 10);
  for (const x of resting) assert.equal(markShows((STARTS[1] - x.qfrom) / (x.qto - x.qfrom), N), 0);
});

