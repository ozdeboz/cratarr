// Sidechaining you can see (decision 0057 §7e; the owner: "as much as you can see", after VCV): a Meter, a Ducker a
// kind keyed by it, each Ducker into the gain input of what it holds back, the Screen's flash guard into the Accent's.
// Open loop, each Ducker gives today's sidechaining's lever exactly, the guard too; closed loop (the frame drawn,
// measured, back in as the Meter's), the picture is over its budgets no longer than under the governor it replaced,
// held back no deeper, flashing never worse.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, track } from './harness/aspects-record.ts';

const { DEFAULTS, FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
const { newEngineV1, stepEngineV1 } = await import('./harness/oracle/engine-v1.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { newSidechain, stepSidechain, BUSY } = await import('../src/lib/visuals/engine/sidechain.ts');
// The governor these replaced (the old sidechaining, kept as the proofs' reference).
const { newGovernor, stepGovernor } = await import('./harness/governor.ts');
const { STANDING } = await import('../src/lib/visuals/engine/sidechain.ts');
const { Meters } = await import('../src/lib/visuals/meters.ts');
const { measure } = await import('./harness/oracle/measure.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { BUDGETED, budgetOf } = await import('../src/lib/visuals/budget.ts');
const { routeKey } = await import('../src/lib/visuals/routes.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;
type CableV2 = import('../src/lib/visuals/ports.ts').CableV2;
type MeterValues = import('../src/lib/visuals/meters.ts').MeterValues;
type AspectsNow = import('../src/lib/visuals/aspects.ts').AspectsNow;
type Levers = import('../src/lib/visuals/engine/sidechain.ts').Levers;

/** Each kind: its Meter output, its budget's meter, what it holds back (a module's knob's gain input; null: the
 *  moments' Gates). */
const KINDS = [
  { kind: 'motion', meter: 'motion', into: ['clock', 'speed'] },
  { kind: 'shape', meter: 'shapeChurn', into: ['flame', 'evolution'] },
  { kind: 'colour', meter: 'colourChurn', into: ['flame', 'hue'] },
  { kind: 'zoom', meter: 'zoomPumping', into: ['flame', 'zoom'] },
  { kind: 'looks', meter: 'looksPerMinute', into: null },
] as const;
const wildOf = (wild: number) => 2 ** (2 * (wild - 0.5));
const one = (id: string, from: CableV2['from'], to: CableV2['to'], value = 1): CableV2 => ({ id, from, to, value, mode: 'swing', depth: null });

/** The sidechains as toV2 is to wire them (shapes to the app); `activity` a number: the thresholds at
 *  that activity instead of the music's (the Envelope's level through a VCA into each threshold's gain). */
function sidechained(p: PatchV2, wild: number, activity: number | false): PatchV2 {
  const modules = [...p.modules, { id: 'meter', type: 'meter', knobs: {} }];
  if (!modules.some((m) => m.type === 'screen')) modules.push({ id: 'screen', type: 'screen', knobs: {} });
  const cables = [...p.cables, one('guard', { module: 'screen', port: 'guard' }, { module: 'accent', port: 'accent:gain' })];
  for (const { kind, meter, into } of KINDS) {
    const most = BUSY[meter].most;
    const id = `duck-${kind}`;
    const threshold = STANDING[meter] * wildOf(wild) * (activity === false ? 1.5 : 0.5 + activity);
    modules.push({ id, type: 'ducker', knobs: { threshold, depth: Math.abs(20 * Math.log10(most)), attack: 1 / 0.7, hold: 0, release: 10 / 3, hysteresis: 0.8 } });
    cables.push(one(`${id}-key`, { module: 'meter', port: kind }, { module: id, port: 'key' }));
    if (activity === false) {
      modules.push({ id: `${id}-activity`, type: 'vca', knobs: { gain: 1 / 3 } });
      cables.push(one(`${id}-loudness`, { module: 'envelope', port: 'level' }, { module: `${id}-activity`, port: 'cv' }, 2 / 3));
      cables.push(one(`${id}-activity-out`, { module: `${id}-activity`, port: 'out' }, { module: id, port: 'threshold:gain' }));
    }
    const targets = into ? [into] : p.modules.filter((m) => m.type === 'gate').map((m) => [m.id, 'gap'] as const);
    for (const [module, knob] of targets) cables.push(one(`${id}-${module}`, { module: id, port: 'out' }, { module, port: `${knob}:gain` }));
  }
  return { ...p, modules, cables };
}

/** Busy on purpose (as tests/sidechain.test.ts): the meters go over and the sidechains have work. */
const busy = structuredClone(DEFAULTS);
Object.assign(busy, { flash: 2, speed: 2 });
busy.routes = {
  ...busy.routes,
  [routeKey('beat', null, 'brightness')]: 1,
  [routeKey('beat', null, 'size')]: 0.8,
  [routeKey('hits', 0, 'lights')]: 1,
  ...Object.fromEntries([0, 1, 2, 3, 4, 5].flatMap((b) => [[routeKey('level', b, 'rotation'), 1], [routeKey('flux', b, 'hue'), 1]])),
};
const DT = 1 / 30;
const FRAMES = 90 * 30;
const leversOf = (o: Record<string, number>): Levers => ({
  swing: o['screen.guard'] ?? 1,
  pace: o['duck-motion.out'] ?? 1,
  evolution: 1 / (o['duck-shape.out'] ?? 1),
  hue: o['duck-colour.out'] ?? 1,
  size: o['duck-zoom.out'] ?? 1,
  gap: 1 / (o['duck-looks.out'] ?? 1),
});

test("open loop: each Ducker gives the lever today's sidechaining gives, the Screen's guard the swing, frame for frame", () => {
  const activity = 0.6;
  for (const id of TRACKS) {
    const t = track(id);
    // The meters a closed loop of today's makes, to key both with.
    const st = newEngineV1();
    const music = newMusicState();
    const meters = new Meters();
    const sc = newSidechain();
    let levers: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };
    const seen: MeterValues[] = [];
    for (let k = 0; k < FRAMES; k++) {
      const at = 20 + k * DT;
      const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: busy.lighting });
      const a = stepEngineV1(st, m, busy, DT, { still: false, now: at * 1000, fluidFull: FLUID_FULL, levers });
      measure(meters, a, k * DT, busy);
      seen.push({ ...meters.now() });
      levers = stepSidechain(sc, seen[k], { dt: DT, limits: STANDING, wild: busy.wild, activity: a.lit, budgets: true });
    }
    // Both keyed by those meters, at one activity.
    const p = sidechained(flatten(toV2(busy, { sidechain: false })), busy.wild, activity);
    const eng = newEngine();
    const ref = newSidechain();
    const music2 = newMusicState();
    let worked = 0;
    for (let k = 0; k < FRAMES; k++) {
      const at = 20 + k * DT;
      const m = musicOf(t.signals, t.beats, at, true, { state: music2, at: at * 1000, lighting: busy.lighting });
      const f = stepEngine(eng, p, {}, m, () => 0, DT, { still: false, now: at * 1000, fluidFull: FLUID_FULL, meters: seen[k] });
      const want = stepSidechain(ref, seen[k], { dt: DT, limits: STANDING, wild: busy.wild, activity, budgets: true });
      const got = leversOf(f.outputs);
      for (const lever of Object.keys(want) as (keyof Levers)[]) {
        assert.ok(Math.abs(got[lever] - want[lever]) <= 1e-9 * Math.max(1, want[lever]), `${id} frame ${k} ${lever}: ${got[lever]} against ${want[lever]}`);
        if (want[lever] !== 1) worked++;
      }
    }
    assert.ok(worked > 100, `${id}: the sidechains held something back (${worked} lever-frames)`);
  }
});

type Run = { over: Record<string, number>; held: Record<string, number>; flashOver: number };
/** A closed loop over a recorded track: each frame drawn, measured, its measures back in for the next. */
function outcome(t: ReturnType<typeof track>, frame: (m: ReturnType<typeof musicOf>, at: number, meters: MeterValues | undefined) => { a: AspectsNow; levers: Levers }): Run {
  const music = newMusicState();
  const meters = new Meters();
  const over: Record<string, number> = {};
  const held: Record<string, number> = {};
  let flashOver = 0;
  let frames = 0;
  let last: MeterValues | undefined;
  for (let k = 0; k < FRAMES; k++) {
    const at = 20 + k * DT;
    const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: busy.lighting });
    const { a, levers } = frame(m, at, last);
    measure(meters, a, k * DT, busy);
    const values = meters.now();
    last = { ...values };
    if (k * DT < 5) continue;
    frames++;
    for (const key of BUDGETED) if (values[key] > budgetOf(STANDING[key], busy.wild, a.lit)) over[key] = (over[key] ?? 0) + DT;
    if (Math.max(values.flash, values.redFlash) > 3) flashOver += DT;
    for (const [key, v] of Object.entries(levers)) held[key] = (held[key] ?? 0) + Math.abs(Math.log(v));
  }
  for (const key of Object.keys(held)) held[key] /= frames;
  return { over, held, flashOver };
}

test('closed loop: the visible sidechains keep the picture within its budgets as the governor did, held back no deeper, flashing never worse', () => {
  for (const id of TRACKS) {
    const t = track(id);
    // The governor it replaced, over today's engine (its levers from the last frame's measures, as the visuals had it).
    const st = newEngineV1();
    const g = newGovernor();
    let levers: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };
    let lit = 0.5;
    const before = outcome(t, (m, at, meters) => {
      if (meters) levers = { ...stepGovernor(g, meters, { dt: DT, limits: STANDING, wild: busy.wild, activity: lit, budgets: true }) };
      const a = stepEngineV1(st, m, busy, DT, { still: false, now: at * 1000, fluidFull: FLUID_FULL, levers });
      lit = a.lit;
      return { a, levers: { ...levers } };
    });
    // The visible wiring over the one engine.
    // toV2 as it ships: its own sidechain wiring (this function's, activity from the music).
    const p = flatten(toV2(busy));
    const eng = newEngine();
    const after = outcome(t, (m, at, meters) => {
      const f = stepEngine(eng, p, {}, m, () => 0, DT, { still: false, now: at * 1000, fluidFull: FLUID_FULL, meters });
      return { a: f.aspects, levers: leversOf(f.outputs) };
    });
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
