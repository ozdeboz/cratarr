// A hold's window centred on its crosshair (0.13, the owner: "It doesn't focus on the crosshair enough. It seems as if
// the window is growing faster than it can keep the crosshair in the center … It should be quite fast, set the
// crosshair in the center to keep it stable. Right now, it lags behind and it also glitches a bit"). Frame by frame,
// lib/waveView's stepWindow and lib/waveStrip's planStrip as lib/SeekWave and lib/Waveform run them: a wind-up to the
// top speed, its crosshair's lead growing with it (the brake's travel), the let go's brake to where it aimed.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { stepWindow, AIM_JUMP } = await import('../src/lib/waveView.ts');
const { planStrip } = await import('../src/lib/waveStrip.ts');

const FRAME = 1000 / 60;
const N = 130;
/** The track: long, so a hold goes on through the queue with no wall but where a test puts one. */
const LEN = 3000;
/** The brake's length (s): letting go at `v`× lands this far on (a constant deceleration to 1×). */
const BRAKE = 0.8;
const travel = (v: number) => ((v + 1) / 2) * BRAKE;

type Frame = { t: number; held: boolean; pos: number; aim: number | null; from: number; to: number; off: boolean; strip: ReturnType<typeof planStrip> };

/**
 * A hold from `pos0` s into a track (the queue's ends walls where `ends`), `dir` its way: 1× up to `top`× over `upS` s,
 * held to `heldS`, let go and braked to where it aimed, then on at 1× for `afterS`. The crosshair: held, where letting
 * go lands; let go, where it aimed, kept until the tape is there (lib/localPlayer's aim). `aimFrom`: the crosshair not
 * known before then (s).
 */
function wind(o: { pos0: number; dir?: 1 | -1; top?: number; upS?: number; heldS: number; afterS?: number; ends?: { lo: number; hi: number }; aimFrom?: number }) {
  const dir = o.dir ?? 1;
  const top = o.top ?? 128;
  const upS = o.upS ?? 4;
  let win: ReturnType<typeof stepWindow> = null;
  let had: ReturnType<typeof planStrip> | null = null;
  let pos = o.pos0;
  let v = 1;
  let aimed: number | null = null;
  let braking: { from: number; v0: number; t0: number } | null = null;
  const out: Frame[] = [];
  const clamp = (x: number) => (o.ends ? Math.max(o.ends.lo, Math.min(o.ends.hi, x)) : x);
  for (let t = 0; t < o.heldS + BRAKE + (o.afterS ?? 3); t += FRAME / 1000) {
    const held = t < o.heldS;
    if (held) {
      v = 1 + (top - 1) * Math.min(1, t / upS) ** 2;
      pos = clamp(pos + dir * v * (FRAME / 1000));
    } else {
      braking ??= { from: pos, v0: v, t0: t };
      const u = Math.min(BRAKE, t - braking.t0);
      // From v0 to 1× over BRAKE at a constant deceleration, then on at 1×.
      const d = braking.v0 * u - ((braking.v0 - 1) / (2 * BRAKE)) * u * u + Math.max(0, t - braking.t0 - BRAKE);
      pos = clamp(braking.from + dir * d);
      v = t - braking.t0 < BRAKE ? braking.v0 - ((braking.v0 - 1) * u) / BRAKE : 1;
    }
    const known = t >= (o.aimFrom ?? 0);
    if (held) aimed = known ? clamp(pos + dir * travel(v)) : null;
    win = stepWindow(win, { len: LEN, pos, moved: 0, holding: held, through: true, dir, speed: v, top, aim: aimed, ends: o.ends }, FRAME);
    if (!win) break;
    had = planStrip(had, [{ key: 't', start: 0, d: LEN, bins: null }], win.shown.from, win.shown.to, N, null);
    out.push({ t, held, pos, aim: aimed, from: win.shown.from, to: win.shown.to, off: !!win.off, strip: had });
  }
  return out;
}
const mid = (f: Frame) => (f.from + f.to) / 2;
const bar = (f: Frame) => (f.to - f.from) / N;

test('an accelerating hold, away from walls: the crosshair in the middle within half a bar, every frame once the hold has begun', () => {
  const run = wind({ pos0: 1500, heldS: 6 });
  const held = run.filter((f) => f.held);
  // The hold's beginning (from the whole track it showed, here a long one) is the offset's, eased away quickly: the
  // crosshair within half a bar of the middle in well under a second, and from then on every frame.
  const settled = held.findIndex((f) => Math.abs(mid(f) - f.aim!) <= bar(f) / 2);
  assert.ok(settled >= 0 && held[settled].t < 0.9, `centred at ${held[settled]?.t}`);
  for (const f of held.slice(settled)) {
    assert.ok(Math.abs(mid(f) - f.aim!) <= bar(f) / 2, `t ${f.t.toFixed(3)}: the crosshair ${(f.aim! - mid(f)).toFixed(3)} s off the middle (a bar ${bar(f).toFixed(3)})`);
  }
  // It does grow through the wind-up: the speed's width, from 20 s to 128× its 15 s.
  assert.ok(held.at(-1)!.to - held.at(-1)!.from > 1900);
});

test('backwards the same: the middle on the crosshair through the wind-up', () => {
  const run = wind({ pos0: 1500, dir: -1, heldS: 5 });
  const held = run.filter((f) => f.held);
  const settled = held.findIndex((f) => Math.abs(mid(f) - f.aim!) <= bar(f) / 2);
  assert.ok(settled >= 0 && held[settled].t < 0.9);
  for (const f of held.slice(settled)) assert.ok(Math.abs(mid(f) - f.aim!) <= bar(f) / 2, `t ${f.t.toFixed(3)}`);
});

test('let go, the middle stays on where it aimed through the brake, then goes on with the playhead: no frame jumps', () => {
  const run = wind({ pos0: 1500, heldS: 4, afterS: 2 });
  const brake = run.filter((f) => !f.held && f.pos < f.aim! - 1e-9);
  assert.ok(brake.length > 30, 'a brake of frames');
  for (const f of brake) assert.ok(Math.abs(mid(f) - f.aim!) <= bar(f) / 2, `t ${f.t.toFixed(3)}: braking, ${(f.aim! - mid(f)).toFixed(3)} off`);
  // The middle moves no more in a frame than the tape and the crosshair's lead do, and a bar (the width's spring out
  // moves the edges, not the middle).
  const from = run.findIndex((f) => !f.off);
  for (let i = from + 1; i < run.length; i++) {
    const [p, q] = [run[i - 1], run[i]];
    const lead = (f: Frame) => (f.aim !== null && (f.aim - f.pos) > 0 ? f.aim - f.pos : 0);
    const tape = Math.abs(q.pos - p.pos) + Math.abs(lead(q) - lead(p));
    assert.ok(Math.abs(mid(q) - mid(p)) <= tape + bar(q), `t ${q.t.toFixed(3)}: the middle moved ${(mid(q) - mid(p)).toFixed(3)} s, the tape ${tape.toFixed(3)}`);
  }
});

test("against the queue's start the window stands on it, and only there is the crosshair off the middle", () => {
  const ends = { lo: -200, hi: LEN };
  const run = wind({ pos0: 50, dir: -1, heldS: 5, ends });
  const held = run.filter((f) => f.held);
  let walled = 0;
  for (const f of held.slice(held.findIndex((f) => !f.off))) {
    assert.ok(f.from >= ends.lo - 1e-9, `t ${f.t.toFixed(3)}: before the queue`);
    if (Math.abs(mid(f) - f.aim!) > bar(f) / 2) {
      walled++;
      assert.ok(f.from - ends.lo <= 0.03 * (f.to - f.from), `t ${f.t.toFixed(3)}: off the middle away from the wall`);
    }
  }
  assert.ok(walled > 0, 'it reached the wall');
});

test('the same number of bars across the window through the whole zoom, the let go and its zoom out', () => {
  const run = wind({ pos0: 1500, heldS: 5, afterS: 4 });
  for (const f of run) {
    const shown = (f.to - f.from) / f.strip.span;
    assert.ok(Math.abs(shown - N) <= 0.25 + 1e-6, `t ${f.t.toFixed(3)}: ${shown.toFixed(3)} bars shown`);
  }
});

test('a crosshair known only later in the hold is come to by the offset, eased: no frame jumps', () => {
  const run = wind({ pos0: 1500, heldS: 4, aimFrom: 1 });
  const at = run.findIndex((f) => f.aim !== null);
  for (let i = Math.max(1, at - 2); i < at + 60; i++) {
    const [p, q] = [run[i - 1], run[i]];
    assert.ok(Math.abs(q.from - p.from) <= Math.abs(q.pos - p.pos) + (q.to - q.from) * 0.05, `t ${q.t.toFixed(3)}: the left edge jumped ${(q.from - p.from).toFixed(3)}`);
  }
  const last = run.filter((f) => f.held).at(-1)!;
  assert.ok(Math.abs(mid(last) - last.aim!) <= bar(last) / 2, 'centred again');
  // The run itself, a wind-up to 128×, never moves the crosshair's lead a jump's worth in a frame.
  const full = wind({ pos0: 1500, heldS: 4 });
  for (let i = 2; i < full.length; i++) {
    const [p, q] = [full[i - 1], full[i]];
    if (!p.held || !q.held) continue;
    assert.ok(Math.abs(q.aim! - q.pos - (p.aim! - p.pos)) < AIM_JUMP * (q.to - q.from), `t ${q.t.toFixed(3)}`);
  }
});
