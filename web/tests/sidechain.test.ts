// Standard sidechaining (engine/sidechain.ts, decision 0057) against the levers it replaces (governor.ts), by outcome
// (agreed with the owner: a ducker cannot follow the old bang-bang integrator frame for frame): over the
// recorded tracks, closed loop (the frame drawn, metered as the precheck meters it, the levers back into the next
// frame), each meter is over its budget no longer than before, the hold-back on average no deeper (the flash guard
// apart: it is safety, and stricter), and a strobe is held down at once and for as long as the rule says.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, track } from './harness/aspects-record.ts';

const { DEFAULTS, FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
const { newEngineV1, stepEngineV1 } = await import('../src/lib/visuals/engine/v1.ts');
const { newSidechain, stepSidechain, FLASH_GUARD } = await import('../src/lib/visuals/engine/sidechain.ts');
// The governor these replaced (the old sidechaining, kept as the proofs' reference).
const { newGovernor, stepGovernor } = await import('./harness/governor.ts');
const { STANDING } = await import('../src/lib/visuals/engine/sidechain.ts');
const { Meters } = await import('../src/lib/visuals/meters.ts');
const { measure } = await import('../src/lib/visuals/precheck.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { BUDGETED, budgetOf } = await import('../src/lib/visuals/budget.ts');
const { routeKey } = await import('../src/lib/visuals/routes.ts');
import type { Levers } from '../src/lib/visuals/engine/sidechain.ts';

/** Busy on purpose: a strong beat into the glow and the size, the bands into the turning and the hues, Flash and Speed
 *  up, so the meters go over and the levers have work. */
const busy = structuredClone(DEFAULTS);
Object.assign(busy, { flash: 2, speed: 2 });
busy.routes = {
  ...busy.routes,
  [routeKey('beat', null, 'brightness')]: 1,
  [routeKey('beat', null, 'size')]: 0.8,
  [routeKey('hits', 0, 'lights')]: 1,
  ...Object.fromEntries([0, 1, 2, 3, 4, 5].flatMap((b) => [[routeKey('level', b, 'rotation'), 1], [routeKey('flux', b, 'hue'), 1]])),
};

type Run = { over: Record<string, number>; held: Record<string, number>; flashOver: number; trace: string[] };
function run(t: ReturnType<typeof track>, step: (m: import('../src/lib/visuals/meters.ts').MeterValues, dt: number, activity: number) => Levers): Run {
  const st = newEngineV1();
  const music = newMusicState();
  const meters = new Meters();
  const dt = 1 / 30;
  let levers: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };
  const over: Record<string, number> = {};
  const held: Record<string, number> = {};
  let flashOver = 0;
  let frames = 0;
  const trace: string[] = [];
  for (let k = 0; k < 90 * 30; k++) {
    const at = 20 + k * dt;
    const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: busy.lighting });
    const a = stepEngineV1(st, m, busy, dt, { still: false, now: at * 1000, fluidFull: FLUID_FULL, levers });
    measure(meters, a, k * dt, busy);
    const values = meters.now();
    levers = step(values, dt, a.lit);
    if (k % 15 === 0 && process.env.TRACE)
      trace.push(`${(k * dt).toFixed(1)} motion ${values.motion.toFixed(2)}/${budgetOf(STANDING.motion, busy.wild, a.lit).toFixed(2)} pace ${levers.pace.toFixed(2)}`);
    if (k * dt < 5) continue;
    frames++;
    for (const key of BUDGETED) if (values[key] > budgetOf(STANDING[key], busy.wild, a.lit)) over[key] = (over[key] ?? 0) + dt;
    if (Math.max(values.flash, values.redFlash) > 3) flashOver += dt;
    for (const [key, v] of Object.entries(levers)) held[key] = (held[key] ?? 0) + Math.abs(Math.log(v)) / 1;
  }
  for (const key of Object.keys(held)) held[key] /= frames;
  return { over, held, flashOver, trace };
}

test('closed loop over the recorded tracks: over budget no longer, held back no deeper (the flash guard apart)', async () => {
  for (const id of TRACKS) {
    const t = track(id);
    const g = newGovernor();
    const before = run(t, (m, dt, activity) => ({ ...stepGovernor(g, m, { dt, limits: STANDING, wild: busy.wild, activity, budgets: true }) }));
    const s = newSidechain();
    const after = run(t, (m, dt, activity) => stepSidechain(s, m, { dt, limits: STANDING, wild: busy.wild, activity, budgets: true }));
    if (process.env.TRACE) {
      const { writeFileSync } = await import('node:fs');
      writeFileSync(`trace-${id}.txt`, before.trace.map((b, i) => `${b.padEnd(44)} | ${after.trace[i]}`).join('\n'));
    }
    for (const key of BUDGETED) {
      const was = before.over[key] ?? 0;
      const now = after.over[key] ?? 0;
      assert.ok(now <= was + 1, `${id} ${key}: ${now.toFixed(1)} s over against ${was.toFixed(1)} s`);
    }
    for (const lever of ['pace', 'evolution', 'hue', 'size', 'gap'] as const)
      assert.ok(after.held[lever] <= before.held[lever] + 0.05, `${id} ${lever}: held ${after.held[lever].toFixed(3)} against ${before.held[lever].toFixed(3)}`);
    assert.ok(after.flashOver <= before.flashOver + 0.5, `${id}: flashing over three a second ${after.flashOver.toFixed(1)} s against ${before.flashOver.toFixed(1)} s`);
  }
});

test('the flash guard: a strobe held to a tenth at once, held 8 s after it stops, a trial included, no knob in it', () => {
  const s = newSidechain();
  const quiet = { flash: 0, redFlash: 0, motion: 0, shapeChurn: 0, colourChurn: 0, zoomPumping: 0, looksPerMinute: 0 } as unknown as import('../src/lib/visuals/meters.ts').MeterValues;
  const strobe = { ...quiet, flash: 6 };
  const opts = { dt: 1 / 30, limits: STANDING, wild: 1, activity: 1, budgets: false };
  let l = stepSidechain(s, strobe, opts);
  assert.ok(Math.abs(l.swing - 0.1) < 1e-9, `at once: ${l.swing}`);
  for (let k = 0; k < 30 * 3; k++) l = stepSidechain(s, strobe, opts);
  assert.ok(Math.abs(l.swing - 0.1) < 1e-9);
  for (let k = 0; k < 30 * 7.9; k++) l = stepSidechain(s, quiet, opts);
  assert.ok(Math.abs(l.swing - 0.1) < 1e-9, `held 8 s: ${l.swing}`);
  for (let k = 0; k < 30 * 15; k++) l = stepSidechain(s, quiet, opts);
  assert.ok(l.swing > 0.95, `let back: ${l.swing}`);
  assert.ok(Object.isFrozen(FLASH_GUARD) || FLASH_GUARD.threshold === 2.5);
});
