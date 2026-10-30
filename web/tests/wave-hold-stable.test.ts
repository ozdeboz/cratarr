// A continuous hold, frame by frame (0.11.1, the owner: "there is a lot of glitching in the waveform on continuous hold
// … the waveforms are entirely unstable, and possibly the auto gain fill is also rapidly changing. So we need everything
// a whole lot more stable, window included."): the real window (lib/waveView's stepWindow) and strips
// (lib/waveStrip's planStrips) as lib/SeekWave and lib/Waveform run them, through a queue, from the wind-up to the
// let go. Measured on 0.11.0: a moment of music's bar changed 668 times in 35 s (by up to a fifth of the height), the
// strip cross-faded 11 times, the window lurched 1.5 s in a frame at the let go and went from standing to the full
// speed in a frame leaving a wall.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { stepWindow } = await import('../src/lib/waveView.ts');
const { planStrips, levelBars, STRIP_SPAN } = await import('../src/lib/waveStrip.ts');

const LENS = [212, 187, 245, 301, 176, 233, 198, 264];
const STARTS = LENS.map((_, i) => LENS.slice(0, i).reduce((a, b) => a + b, 0));
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
const N = 333;

type Seen = { t: number; from: number; to: number; strips: { L: number; g0: number; pitch: number; bars: number[]; share: number }[] };

/**
 * A hold from `q0` s along the queue: 4x doubling every 2 s to 16x, held `heldS` s, then braked to 1x in a second and
 * on for `afterS` s. Each frame: the window along the queue, and the strips shown with their bars and shares.
 */
function hold(q0: number, heldS: number, afterS: number, through = true): Seen[] {
  let win: ReturnType<typeof stepWindow> = null;
  let base: number | null = null;
  let had: ReturnType<typeof planStrips>['strips'] = [];
  const bars = new Map<object, number[]>();
  let q = q0;
  let v = 1;
  let idx = -1;
  const out: Seen[] = [];
  for (let t = 0; t < heldS + afterS; t += FRAME / 1000) {
    const holding = t < heldS;
    v = holding ? Math.min(16, 4 * 2 ** (t / 2)) : Math.max(1, v - 15 * (FRAME / 1000));
    q += v * (FRAME / 1000);
    let i = 0;
    while (i < LENS.length - 1 && q >= STARTS[i + 1]) i++;
    const moved = idx < 0 || idx === i ? 0 : STARTS[idx] - STARTS[i];
    idx = i;
    const pos = q - STARTS[i];
    const was = win;
    win = stepWindow(win, { len: LENS[i], pos, moved, holding, through, dir: 1, speed: v, top: 16, aim: holding ? pos + (v * v) / 128 : null }, FRAME);
    if (!win) continue;
    if (!was) base = i;
    // lib/SeekWave's hold frame: s from the start of the track the hold began in; the tracks the strips cover.
    const at = STARTS[base!] - STARTS[i];
    const reach = ((STRIP_SPAN - 1) / 2) * (win.shown.to - win.shown.from);
    const tracks = LENS.map((d, k) => ({ key: `t${k}`, start: STARTS[k] - STARTS[i] - at, d, bins: BINS[k] })).filter(
      (x) => through || x.key === `t${i}`,
    );
    const from = win.shown.from - at;
    const to = win.shown.to - at;
    const shown = tracks.filter((x) => x.start < to + reach && x.start + x.d > from - reach);
    const plan = planStrips(had, shown, from, to, N, 1 / 64);
    had = plan.strips;
    out.push({
      t,
      from: win.shown.from + STARTS[i],
      to: win.shown.to + STARTS[i],
      strips: plan.strips.map((s, k) => {
        if (!bars.has(s)) bars.set(s, levelBars(shown, s));
        return { L: s.L, g0: s.g0, pitch: s.pitch, bars: bars.get(s)!, share: plan.strips.length < 2 ? 1 : k === 0 ? 1 - plan.mix : plan.mix };
      }),
    });
  }
  return out;
}

const run = hold(30, 30, 5);

test('a moment of music draws the same bar all through the hold, at each level: nothing redraws it otherwise', () => {
  const seen = new Map<string, number>();
  let checked = 0;
  for (const f of run)
    for (const s of f.strips)
      for (let i = 0; i < s.bars.length; i += 7) {
        const key = `${s.L}:${s.g0 + i}`;
        const was = seen.get(key);
        if (was !== undefined) {
          assert.equal(s.bars[i], was, `${f.t.toFixed(2)} s: level ${s.L}'s bar ${s.g0 + i} changed`);
          checked++;
        }
        seen.set(key, s.bars[i]);
      }
  assert.ok(checked > 10000);
});

test('the zoom fades its levels into each other a little each frame: no cross-fade at a moment, none popping in', () => {
  let last = new Map<number, number>();
  for (const [k, f] of run.entries()) {
    const now = new Map(f.strips.map((s) => [s.L, s.share]));
    if (k > 0)
      for (const L of new Set([...now.keys(), ...last.keys()])) {
        const d = Math.abs((now.get(L) ?? 0) - (last.get(L) ?? 0));
        assert.ok(d <= 0.1, `${f.t.toFixed(2)} s: level ${L}'s share moved ${d.toFixed(3)} in a frame`);
      }
    last = now;
  }
});

/** How much the window's motion changes from one frame to the next (s of the queue per frame, per frame). */
function jolts(frames: Seen[]): { t: number; d: number; step: number }[] {
  const out = [];
  for (let k = 2; k < frames.length; k++) {
    const a = frames[k].from - frames[k - 1].from;
    const b = frames[k - 1].from - frames[k - 2].from;
    out.push({ t: frames[k].t, d: Math.abs(a - b), step: a });
  }
  return out;
}

test('the window never lurches: from the hold to the let go its motion changes a little each frame', () => {
  // 0.11.0: 1.48 s of the queue in a frame at the let go (its handing over eased away fastest at first).
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
  // 0.11.0: from standing to 0.27 s a frame in one frame.
  for (const j of leaving) assert.ok(j.d <= 0.03, `${j.t.toFixed(2)} s: ${j.d.toFixed(3)} s a frame at once`);
  assert.ok(leaving.at(-1)!.step > 0.2, 'then at the tape\'s speed');
});
