// A hold's window on the waveform (lib/waveView; 0.10.1): continuous by construction, the crosshair converged on to
// exactly the middle, the edge a direction does not need pinned (the owner: "Wherever a direction doesn't need the
// neighbour, it's not shown, and the edge never moves").
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { holdWalls, place, ease, settleOf, offsetOf, easeOffset, shownOf, waveColours, stepWindow, ZOOM_S, ZOOM_KEEP_MS } = await import('../src/lib/waveView.ts');

const LEN = 240;
const FRAME = 1000 / 60;
const FOCUS_MS = 300;
const WIDTH_MS = 250;
const WIDE_MS = 400;

type Frame = { at: number; aim: number; from: number; to: number; rel: number; w: number; oa: number; ob: number };
const placeOf = (x: number, w: { from: number; to: number }) => (x - w.from) / (w.to - w.from);

/**
 * A hold as lib/SeekWave runs it, 60 frames a second, in s from this track's start (the queue's tracks before at
 * negative places): the tape from `at0` at `speed` (signed, track s per s), the crosshair `lead` s ahead of it the way
 * it goes; the window from the whole track to `width` wide. Through the queue it is the wide view, from the whole
 * track by an offset eased away; else the zoom on this track, walled at both its edges.
 */
function hold(at0: number, speed: number, width: number, through: boolean, lead = 4, secs = 5) {
  const dir = speed > 0 ? 1 : -1;
  const walls = through ? holdWalls(dir, true, 0, LEN) : { lo: 0, hi: LEN };
  const lo = through ? -Infinity : 0;
  const hi = through ? Infinity : LEN;
  const aimOf = (at: number) => Math.min(hi, Math.max(lo, at + dir * lead));
  const out: Frame[] = [];
  let at = at0;
  let w = LEN;
  let rel = aimOf(at0) - at0;
  let off: { a: number; b: number } | null = null;
  for (let f = 0; f < secs * 60; f++) {
    w = ease(w, width, FRAME, WIDTH_MS, settleOf(width));
    at = Math.min(hi, Math.max(lo, at + speed / 60));
    // Stopping at an edge, the crosshair stops there too.
    const aim = aimOf(at);
    rel = ease(rel, aim - at, FRAME, FOCUS_MS, settleOf(w));
    const placed = place(at + rel, w, walls);
    if (through) off = f === 0 ? offsetOf({ from: 0, to: LEN }, placed) : off && easeOffset(off, FRAME, WIDE_MS, settleOf(w));
    out.push({ at, aim, ...shownOf(placed, off, walls), rel, w, oa: off?.a ?? 0, ob: off?.b ?? 0 });
  }
  return out;
}

/** The largest move of either edge between two frames. */
function largestStep(frames: { from: number; to: number }[]): number {
  let m = 0;
  for (let i = 1; i < frames.length; i++) m = Math.max(m, Math.abs(frames[i].from - frames[i - 1].from), Math.abs(frames[i].to - frames[i - 1].to));
  return m;
}
/** No jump: no edge moves more in a frame than the tape, the eased lead, width and offset move it. */
function assertSmooth(frames: Frame[]) {
  for (let i = 1; i < frames.length; i++) {
    const [p, q] = [frames[i - 1], frames[i]];
    const room = Math.abs(q.at - p.at) + Math.abs(q.rel - p.rel) + Math.abs(q.w - p.w);
    assert.ok(Math.abs(q.from - p.from) <= room + Math.abs(q.oa - p.oa) + 1e-9, `frame ${i}: the left edge jumped ${q.from - p.from}`);
    assert.ok(Math.abs(q.to - p.to) <= room + Math.abs(q.ob - p.ob) + 1e-9, `frame ${i}: the right edge jumped ${q.to - p.to}`);
  }
}
/** Settled: the crosshair exactly in the middle (a pixel's fraction). */
const centred = (w: Frame) => Math.abs(placeOf(w.aim, w) - 0.5) < 1e-4;

test('away from the edges the crosshair converges on exactly the middle, without passing it', () => {
  for (const speed of [8, -8]) {
    const f = hold(120, speed, 30, true, 5);
    assert.ok(centred(f.at(-1)!), 'settled exactly in the middle');
    // From one side only: once the width has settled, never past the middle.
    const d = f.slice(60).map((w) => placeOf(w.aim, w) - 0.5);
    const side = Math.sign(d.find((x) => Math.abs(x) > 1e-3) ?? 0);
    for (const x of d) assert.ok(x * side >= -1e-4, 'never past the middle');
    assertSmooth(f);
  }
});

test('the tape at any speed is followed exactly: no lag behind the crosshair once settled', () => {
  const f = hold(60, 30, 60, true, 10, 6);
  for (const w of f.slice(-60)) assert.ok(centred(w));
});

test('holding forward from 0: the start stays the left edge, the crosshair moves right from it to the middle', () => {
  for (const through of [true, false]) {
    const f = hold(0, 6, 30, through);
    for (const w of f) assert.ok(w.from >= 0, 'never the track before');
    // Until the crosshair can stand in the middle (15 s in), the start is the edge.
    for (const w of f.filter((x) => x.aim < 14)) assert.equal(w.from, 0, 'the start edge does not move');
    assert.ok(centred(f.at(-1)!), 'then in the middle');
    assertSmooth(f);
  }
});

test('holding back from 0 through the queue: the crosshair goes to the middle, the track before showing', () => {
  const f = hold(0, -6, 30, true);
  for (const w of f) assert.ok(w.to <= LEN, 'never the track after');
  const late = f.at(-1)!;
  assert.ok(late.from < 0, 'the track before shows');
  assert.ok(centred(late), 'the crosshair in the middle');
  assertSmooth(f);
});

test("holding back from 0 where a hold stops at a track's edge: the start is a wall, the playhead stays at it", () => {
  const f = hold(0, -6, 30, false);
  for (const w of f) {
    assert.equal(w.from, 0, 'the start edge stays');
    assert.equal(placeOf(w.at, w), 0, 'the playhead at the wall');
  }
});

test('holding back near the end: the end stays the right edge, the next track never shows', () => {
  for (const through of [true, false]) {
    const f = hold(LEN - 2, -6, 30, through);
    for (const w of f) assert.ok(w.to <= LEN, 'never past the end');
    for (const w of f.filter((x) => x.aim > LEN - 14)) assert.equal(w.to, LEN);
    assert.ok(centred(f.at(-1)!));
    assertSmooth(f);
  }
});

test('holding forward at the end through the queue: on into the next track, the crosshair in the middle', () => {
  const f = hold(LEN - 2, 6, 30, true);
  assert.ok(f.at(-1)!.to > LEN, 'the next track shows');
  assert.ok(centred(f.at(-1)!));
  assertSmooth(f);
});

test("a hold's first frame is close to the whole track it showed: no jump as it begins", () => {
  for (const [at0, speed, through] of [[0, 6, true], [200, 6, true], [0, -6, true], [230, -6, true], [100, 6, false]] as const) {
    const first = hold(at0, speed, 30, through)[0];
    // The wide view from exactly the whole; the zoom a frame's easing in from it, the walls keeping it on the track.
    if (through) assert.deepEqual([first.from, first.to], [0, LEN], `from ${at0} at ${speed}`);
    else assert.ok(first.from >= 0 && first.to <= LEN && first.to - first.from > LEN * 0.9, `from ${at0} at ${speed}: ${first.from}..${first.to}`);
  }
});

test('the crosshair moving at once (a new landing) is eased, not jumped to', () => {
  let rel = 0;
  const walls = holdWalls(1, true, 0, LEN);
  const at = 100;
  const frames: Frame[] = [];
  for (let f = 0; f < 240; f++) {
    const aim = f < 10 ? at : at + 12;
    rel = ease(rel, aim - at, FRAME, FOCUS_MS, settleOf(30));
    frames.push({ at, aim, ...place(at + rel, 30, walls), rel, w: 30, oa: 0, ob: 0 });
  }
  assert.ok(largestStep(frames) < 1, `the largest step ${largestStep(frames)}`);
  assert.ok(centred(frames.at(-1)!));
});

test('a zoom out near the start keeps the start as the left edge throughout, ending at the whole track', () => {
  const walls = holdWalls(1, false, 0, LEN);
  let w = 20;
  for (let f = 0; f < 600; f++) {
    w = ease(w, LEN, FRAME, 700, settleOf(LEN));
    assert.equal(place(9, w, walls).from, 0);
  }
  assert.deepEqual(place(9, w, walls), { from: 0, to: LEN });
});

test('easing never passes its target and lands on it exactly', () => {
  let x = 0;
  for (let f = 0; f < 200; f++) {
    const next = ease(x, 10, FRAME, FOCUS_MS, 1e-3);
    assert.ok(next >= x && next <= 10);
    x = next;
  }
  assert.equal(x, 10);
});

test("a let go hands over to the track's own view without a jump, settling on it exactly", () => {
  // Wide through the queue, showing the next track; the own view a zoom on this track near its end.
  const walls = holdWalls(1, true, 0, LEN);
  let at = 236;
  const shownThen = place(at + 3, 30, walls);
  const own = (x: number) => place(x, 30, { lo: 0, hi: LEN });
  let off: { a: number; b: number } | null = offsetOf(shownThen, own(at));
  let last = shownThen;
  for (let f = 0; f < 240 && off; f++) {
    at = Math.min(LEN, at + 1 / 60);
    const was = off;
    off = easeOffset(off, FRAME, WIDE_MS, settleOf(30));
    const now = shownOf(own(at), off, walls);
    // Each edge moves by what the tape and the easing move it, no more.
    const room = 1 / 60 + Math.abs((off?.a ?? 0) - was.a) + Math.abs((off?.b ?? 0) - was.b) + 1e-9;
    assert.ok(Math.abs(now.from - last.from) <= room && Math.abs(now.to - last.to) <= room, `frame ${f}`);
    last = now;
  }
  assert.equal(off, null, 'settled');
  assert.deepEqual(last, own(at));
});

test("the waveform's colours: the shown track the player's own, the others their covers', all one where the covers give none", () => {
  const accent = (k: string) => ({ a: '#d4b020', b: '#3080e0' } as Record<string, string>)[k] ?? null;
  assert.deepEqual(waveColours(['a', 'b', 'c'], 'a', true, accent), [null, '#3080e0', 'var(--accent)']);
  // Not from the covers (the mini player's Cover colour off): one colour throughout.
  assert.deepEqual(waveColours(['a', 'b', 'c'], 'a', false, accent), [null, null, null]);
});

// The window as lib/SeekWave runs it, frame by frame (stepWindow), over a queue of tracks.
type Run = { pos: number; along: number; len: number; idx: number; from: number; to: number; w: number; off: boolean } | null;
/**
 * A hold through `lens` (the queue's tracks), from `at0` s into track `idx0`, its speed each frame by `speedAt` (s from
 * the start; signed), held for `heldS` s, the crosshair `leadOf(speed)` s ahead; then let go and the tape at 1× again.
 * Each frame: the window shown in s from the playing track's start, and the tape's place along the queue.
 */
function run(lens: number[], idx0: number, at0: number, speedAt: (t: number) => number, heldS: number, secs: number, through = true, top = 12, leadOf = (v: number) => v / 2) {
  const starts = lens.map((_, i) => lens.slice(0, i).reduce((a, b) => a + b, 0));
  let idx = idx0;
  let pos = at0;
  let win: ReturnType<typeof stepWindow> = null;
  const out: Run[] = [];
  for (let f = 0; f < secs * 60; f++) {
    const t = f / 60;
    const holding = t < heldS;
    const v = holding ? speedAt(t) : 1;
    const was = idx;
    pos += v / 60;
    if (through) {
      while (pos >= lens[idx] && idx < lens.length - 1) pos -= lens[idx++];
      while (pos < 0 && idx > 0) pos += lens[--idx];
    }
    pos = Math.min(lens[idx], Math.max(0, pos));
    const lead = leadOf(Math.abs(v)) * Math.sign(v);
    win = stepWindow(win, { len: lens[idx], pos, moved: starts[was] - starts[idx], holding, through, dir: v > 0 ? 1 : -1, speed: Math.abs(v), top, aim: holding ? pos + lead : null }, 1000 / 60);
    out.push(win ? { pos, along: starts[idx] + pos, len: lens[idx], idx, from: win.shown.from + starts[idx], to: win.shown.to + starts[idx], w: win.w, off: !!win.off } : null);
  }
  return out;
}

test('at the top speed the window stands: the same width in s every frame, across a track boundary, until let go', () => {
  const top = 12;
  // Up to the top speed in a second, then held there for 9 s, through three tracks.
  const f = run([240, 60, 200], 0, 150, (t) => Math.min(top, 1 + t * 11), 10, 10, true, top);
  const settled = f.filter((x, i) => x && i > 4 * 60);
  assert.ok(settled.length > 300);
  const W = top * ZOOM_S;
  for (const x of settled) {
    assert.equal(x!.w, W, 'the width the top speed asks for, exactly');
    assert.ok(Math.abs(x!.to - x!.from - W) < 1e-9, 'shown that wide');
  }
  // Along the queue it moves exactly with the tape, the boundaries included: nothing else moves it.
  const idxs = new Set(settled.map((x) => x!.idx));
  assert.ok(idxs.size >= 2, 'it crossed a boundary');
  for (let i = 1; i < settled.length; i++) {
    const [p, q] = [settled[i - 1]!, settled[i]!];
    assert.ok(Math.abs(q.from - p.from - (q.along - p.along)) < 1e-9, `frame ${i}: the window moved other than the tape`);
  }
  // Never out to the whole track (it was, after 1.5 s at the top speed).
  for (const x of settled) assert.ok(x!.to - x!.from < 240);
});

test('a speed wavering below the top never narrows the window: it only widens in a hold', () => {
  const f = run([400], 0, 100, (t) => (t < 1 ? 1 + t * 10 : 11 + Math.sin(t * 20) * 0.5), 6, 6, true, 12);
  let widest = 0;
  for (const x of f.slice(70)) {
    assert.ok(x!.w >= widest - 1e-9, 'never narrower');
    widest = Math.max(widest, x!.w);
  }
});

test('let go: the window stays, then eases back to the whole track and ends there, never jumping', () => {
  const f = run([240, 200], 0, 200, (t) => Math.min(12, 1 + t * 11), 3, 14, true);
  const last = f.findLastIndex((x) => x !== null);
  assert.ok(last > 3 * 60 && last < f.length - 1, 'it ended');
  const end = f[last]!;
  // The frame before it ends is within a pixel's fraction of the whole (the one exactly on it is none).
  const near = settleOf(end.len) * 2;
  assert.ok(Math.abs(end.from - (end.along - end.pos)) < near && Math.abs(end.to - end.from - end.len) < near, 'the whole track at its end');
  // Kept for ZOOM_KEEP_MS after the let go, as wide as it was.
  const keep = f.slice(3 * 60 + 2, 3 * 60 + Math.floor((ZOOM_KEEP_MS / 1000) * 60) - 2);
  for (const x of keep) assert.equal(x!.w, f[3 * 60 - 1]!.w);
  // No frame jumps: each edge moves less than a fifth of the width a frame (a jump was a track's length).
  for (let i = 1; i <= last; i++) {
    const [p, q] = [f[i - 1]!, f[i]!];
    assert.ok(Math.abs(q.from - p.from) < (q.to - q.from) / 5 && Math.abs(q.to - p.to) < (q.to - q.from) / 5, `frame ${i}`);
  }
});

test('on one track (Start and stop at its edges): no wider than the track, both edges walls', () => {
  const f = run([60], 0, 30, () => 12, 4, 4, false, 12);
  for (const x of f) {
    assert.ok(x!.from >= 0 && x!.to <= 60 + 1e-9);
    assert.ok(x!.w <= 60);
  }
});

test('held forward from 0: the start stays the left edge until the crosshair can stand in the middle', () => {
  const f = run([240, 200], 0, 0, () => 6, 5, 5, true, 12);
  for (const x of f) assert.ok(x!.from >= 0, 'never the track before');
  for (const x of f.filter((x) => x!.pos + 3 < (x!.to - x!.from) / 2 - 1)) assert.equal(x!.from, 0);
});
