// The one engine (decision 0057, step 2): every value and trigger of a v2 patch, a frame at a time, over its typed
// cables. Music, Track and the Envelope first (what every cable reads and what scales it); then the modulators in the
// order their cables need (a cycle reads last frame's value, VCV Rack's rule); then what the cables give each of the
// Cratarr chain's columns, by the routes' rules (routes.ts give: a swing sums, a level lets through, a band's cable into
// a part's column is that part's own, into another the bands' mean, a moment's their largest), each column's knob its
// scale; then the chain's own modules (chain.ts); and every other module's knobs as their cables move them (evaluate.ts's
// rule). What aspects.ts and evaluate.ts did between them, in one place and all of it wired.

import type { AspectsNow } from '../aspects';
import { MOMENT_GAP, heightOf, letThrough, shapeOf, type MomentRequest } from '../evaluate';
import { seedOf } from '../flame';
import type { Levers } from '../governor';
import { MODULATORS, type ModulatorType } from '../modulators/modulators';
import type { Music } from '../music-core';
import { ASPECT_KNOB, type Knob, type Macro, type Module, type ModuleType, type Modulator, type SignalRef } from '../patch';
import type { CableV2, PatchV2, PortRef } from '../ports';
import { A, ASPECTS, B, BAND_SIGNALS, COLUMN_SCALES, LEVEL_ASPECTS, MOMENT_ASPECTS, PART_ASPECTS, T, TRACK_SIGNALS, given, type Aspect, type Given } from '../routes';
import { BANDS, type VisualSettings } from '../settings.svelte';
import { newChain, stepChain, type ChainState } from './chain';
import { newClock, newEnvelope, newTrack, stepClock, stepEnvelope, stepTrack, type ClockState, type EnvelopeState, type TrackState } from './modules';

const NA = ASPECTS.length;
const IS_LEVEL = ASPECTS.map((a) => LEVEL_ASPECTS.includes(a));
const IS_PART = ASPECTS.map((a) => PART_ASPECTS.includes(a));
const IS_MOMENT = ASPECTS.map((a) => (MOMENT_ASPECTS as readonly Aspect[]).includes(a));
/** Each chain knob that is a column (`module|knob` → its place in ASPECTS). */
const COLUMN = new Map<string, number>((Object.entries(ASPECT_KNOB) as [Aspect, [string, string]][]).map(([a, [m, k]]) => [`${m}|${k}`, A[a]]));
/** Each column a setting scales (COLUMN_SCALES; the fluid's stirring is scaled where it stirs). */
const SCALED = (Object.entries(COLUMN_SCALES) as [Aspect, string][]).filter(([a]) => a !== 'stir' && a !== 'flow' && a !== 'swirl').map(([a, k]) => [A[a], k] as const);
/** Halved with reduced motion, as the routes read them. */
const HALVED = new Set(['beat', 'downbeat', 'section']);
const OPEN: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };
/** A macro's value as a knob (0..1), as evaluate.ts has it. */
const MACRO_KNOB: Knob = { id: 'value', label: 'Value', min: 0, max: 1, neutral: 0, default: 0, kind: 'swing' };
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const finite = (v: unknown, or = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : or);

export type EngineState = {
  envelope: EnvelopeState;
  clock: ClockState;
  track: TrackState;
  chain: ChainState;
  /** The track's signals as the cables read them raw (the loudness the Envelope's level, the Track pulse in them). */
  signals: Float32Array;
  standIn: Float32Array[];
  got: Given;
  modulators: Record<string, { type: string; run: Modulator }>;
  /** Each modulator's last output (a cycle reads it). */
  values: Record<string, number>;
  /** The beat last seen (beats into a port count exactly). */
  beat?: number | null;
  /** Each macro's last value; each step's moment (ready, and how long since it last came). */
  macros: Record<string, number>;
  moments: Record<string, { armed: boolean; since: number }>;
};

export function newEngine(): EngineState {
  return {
    envelope: newEnvelope(),
    clock: newClock(),
    track: newTrack(),
    chain: newChain(),
    signals: new Float32Array(TRACK_SIGNALS.length),
    standIn: Array.from({ length: BANDS }, () => {
      const v = new Float32Array(BAND_SIGNALS.length);
      v[B.level] = 0.5;
      return v;
    }),
    got: given(BANDS),
    modulators: {},
    values: {},
    macros: {},
    moments: {},
  };
}

export type EngineFrame = {
  /** What the picture is drawn with this frame (today's renderer's input, until step 3's modules take knob values). */
  aspects: AspectsNow;
  /** Every module's knobs as their cables move them (the chain's: its settings, its columns given in `aspects`). */
  knobs: Record<string, Record<string, number>>;
  /** Every value output this frame, by `module.port`. */
  outputs: Record<string, number>;
  /** Each macro's value this frame. */
  macros: Record<string, number>;
  /** The steps past the chain whose cables ask for a moment (morph.ts takes it at the module's neutral). */
  moments: MomentRequest[];
};

export type EngineOptions = {
  still: boolean;
  now: number;
  fluidFull: number;
  levers?: Levers;
  /** Each walking cable's factor now, by its id (1: as set). */
  walk?: Record<string, number>;
};

/** One frame. `read`: the music's signals on 1 = typical (normal.ts signalsFor), for cables that read them so; `s`: the
 *  chain's knobs as settings (until the chain's modules hold their own). */
export function stepEngine(st: EngineState, p: PatchV2, types: Record<string, ModuleType>, m: Music, read: (ref: SignalRef) => number, s: VisualSettings, dt: number, opts: EngineOptions): EngineFrame {
  const L = opts.levers ?? OPEN;
  const step = Number.isFinite(dt) ? Math.max(0, dt) : 0;
  const typeOf = new Map(p.modules.map((x) => [x.id, x.type]));
  const outputs: Record<string, number> = {};

  // Music's raw signals, the Envelope, the Track's pulse.
  const env = stepEnvelope(st.envelope, m.intensity, m.playing, s.intensity, step);
  const raw = st.signals;
  raw.set(m.track);
  raw[T.loudness] = env.level;
  if (opts.still) for (const t of [T.beat, T.downbeat, T.section]) raw[t] *= 0.5;
  const pulse = stepTrack(st.track, m.key, opts.now, m.bps);
  raw[T.track] = pulse;
  const bands = m.bands ?? st.standIn;
  Object.assign(outputs, { 'envelope.level': env.level, 'envelope.out': env.out, 'envelope.glow': env.glow, 'track.pulse': pulse });
  const index = m.beat?.index ?? null;
  const onBeat = index !== null && st.beat !== undefined && index !== st.beat ? 1 : 0;
  st.beat = index;

  // The modulators running: one each, anew where its type changed, none for those gone.
  const modulatorType = (type: string): ModulatorType | null => {
    const t = types[type] as Partial<ModulatorType> | undefined;
    return t && typeof t.create === 'function' ? (t as ModulatorType) : (MODULATORS[type] ?? null);
  };
  const running = p.modules.filter((x) => modulatorType(x.type));
  const live = new Set(running.map((x) => x.id));
  for (const id of Object.keys(st.modulators))
    if (!live.has(id)) {
      delete st.modulators[id];
      delete st.values[id];
    }
  for (const x of running)
    if (st.modulators[x.id]?.type !== x.type) {
      st.modulators[x.id] = { type: x.type, run: modulatorType(x.type)!.create(seedOf(x.id)) };
      st.values[x.id] = 0;
    }

  /** A cable's source now: a signal raw (absolute, as routes read it) or on 1 = typical halved into 0..1 (as cables
   *  read it otherwise), the Track's pulse, the Envelope's, a modulator's output (a bipolar one from its middle); null
   *  where its module is not there. */
  const xOf = (c: CableV2): number | null => {
    const f = c.from;
    const type = typeOf.get(f.module) ?? f.module;
    if (type === 'music') {
      if (c.from.reference === 'absolute') {
        const t = T[f.port as keyof typeof T];
        if (f.channel === undefined && t !== undefined) return raw[t];
        const b = B[f.port as keyof typeof B];
        return b !== undefined && f.channel !== undefined ? (bands[f.channel]?.[b] ?? 0) * (opts.still && HALVED.has(f.port) ? 0.5 : 1) : 0;
      }
      return clamp(finite(read({ signal: f.port, band: f.channel, reference: c.from.reference, speed: c.from.speed })) / 2, 0, 1);
    }
    if (type === 'track') return pulse;
    if (type === 'envelope') return f.port === 'out' ? env.out : env.level;
    if (type === 'clock') return outputs['clock.rate'] ?? st.clock.rate;
    if (st.modulators[f.module]) {
      const v = f.module in outputs ? outputs[f.module] : finite(st.values[f.module]);
      return clamp(modulatorType(st.modulators[f.module].type)?.range === 'bipolar' ? (v + 1) / 2 : v, 0, 1);
    }
    return null;
  };
  /** What scales a cable: its depth's value (null: nothing, 1; undefined: the Envelope's, normalled). */
  const depthOf = (c: CableV2): number => {
    if (c.depth === null) return 1;
    if (c.depth === undefined) return env.out;
    return clamp(finite(valueAt(c.depth), 1), 0, 1);
  };
  const valueAt = (r: PortRef): number => {
    const key = `${r.module}.${r.port}`;
    if (key in outputs) return outputs[key];
    if (r.module in outputs) return outputs[r.module];
    return finite(st.values[r.module]);
  };
  const walkOf = (c: CableV2) => (shapeOf(c.value).walks ? finite(opts.walk?.[c.id], 1) : 1);

  const into = new Map<string, CableV2[]>();
  for (const c of p.cables) {
    const key = `${c.to.module}|${c.to.port}`;
    (into.get(key) ?? into.set(key, []).get(key)!).push(c);
  }

  // The modulators, in the order their cables need.
  const portSum = (module: string, port: string): number => {
    let sum = 0;
    for (const c of into.get(`${module}|${port}`) ?? []) {
      const sh = shapeOf(c.value);
      const w = walkOf(c);
      const type = typeOf.get(c.from.module) ?? c.from.module;
      if (type === 'music' && c.from.port === 'beat' && c.from.channel === undefined) {
        sum += (sh.table ? 2 * heightOf(sh, onBeat / 2) : sh.end * onBeat) * w;
        continue;
      }
      if (type === 'music') {
        const v = c.from.reference === 'absolute' ? (xOf(c) ?? 0) : finite(read({ signal: c.from.port, band: c.from.channel, reference: c.from.reference, speed: c.from.speed }));
        sum += (sh.table ? 2 * heightOf(sh, clamp(v / 2, 0, 1)) : sh.end * v) * w;
        continue;
      }
      const x = xOf(c);
      if (x !== null) sum += heightOf(sh, x) * w;
    }
    return sum;
  };
  // The macros' targets: a target's base lerp(from, to, the macro's value) before its cables act (not a step's).
  const macroIds = new Set(p.macros.map((x) => x.id));
  for (const id of Object.keys(st.macros)) if (!macroIds.has(id)) delete st.macros[id];
  const targets = new Map<string, { macro: Macro; from: number; to: number }[]>();
  for (const macro of p.macros)
    for (const t of macro.targets) {
      const key = `${t.module}|${t.knob}`;
      (targets.get(key) ?? targets.set(key, []).get(key)!).push({ macro, from: t.from, to: t.to });
    }
  const macros: Record<string, number> = {};
  const macroOf = (x: Macro) => (x.id in macros ? macros[x.id] : finite(st.macros[x.id], clamp(finite(x.value), 0, 1)));
  const moments: MomentRequest[] = [];
  const modulesNow = new Set(p.modules.map((x) => x.id));
  for (const key of Object.keys(st.moments)) if (!modulesNow.has(key.slice(0, key.indexOf('|')))) delete st.moments[key];

  // The macros and the modulators, in the order their cables (and a modulator's macros) need.
  const knobs: Record<string, Record<string, number>> = {};
  type Node = { id: string; macro?: Macro; module?: Module };
  const nodes: Node[] = [...p.macros.map((macro) => ({ id: macro.id, macro })), ...running.map((module) => ({ id: module.id, module }))];
  const ids = new Set(nodes.map((x) => x.id));
  const needs = new Map<string, Set<string>>(nodes.map((x) => [x.id, new Set<string>()]));
  for (const c of p.cables) if (ids.has(c.from.module) && ids.has(c.to.module) && c.from.module !== c.to.module) needs.get(c.to.module)!.add(c.from.module);
  for (const macro of p.macros) for (const t of macro.targets) if (ids.has(t.module) && !macroIds.has(t.module)) needs.get(t.module)!.add(macro.id);
  const done = new Set<string>();
  while (done.size < nodes.length) {
    const next = nodes.find((x) => !done.has(x.id) && [...needs.get(x.id)!].every((d) => done.has(d))) ?? nodes.find((x) => !done.has(x.id))!;
    done.add(next.id);
    if (next.macro) {
      macros[next.id] = moved(next.id, MACRO_KNOB, clamp(finite(next.macro.value), 0, 1), into.get(`${next.id}|value`) ?? []);
      continue;
    }
    const x = next.module!;
    const t = modulatorType(x.type)!;
    const k = knobsOf(x, t);
    for (const port of t.inputs ?? []) k[port] = portSum(x.id, port);
    knobs[x.id] = k;
    outputs[x.id] = st.modulators[x.id].run.step(step, read, k);
  }
  for (const id of Object.keys(st.modulators)) if (id in outputs) st.values[id] = outputs[id];
  for (const id of Object.keys(macros)) st.macros[id] = macros[id];

  // The chain's columns: what their cables give, by the routes' rules.
  const got = st.got;
  got.whole.fill(0);
  got.level.fill(1);
  for (const own of got.band) own.fill(0);
  const n = got.band.length;
  const bandLevel = Array.from({ length: n }, () => new Float64Array(NA).fill(1));
  for (const [key, cables] of into) {
    const a = COLUMN.get(key);
    if (a === undefined) continue;
    for (const c of cables) {
      const x = xOf(c);
      if (x === null) continue;
      const band = c.from.channel ?? null;
      if (band !== null && !(band >= 0 && band < n)) continue;
      const sh = shapeOf(c.value);
      const y = heightOf(sh, x);
      const w = walkOf(c);
      const e = depthOf(c);
      if (band === null) {
        if (IS_LEVEL[a]) got.level[a] *= letThrough(sh.peak, y, e, w);
        else got.whole[a] += e * y * w;
      } else if (IS_LEVEL[a]) bandLevel[band][a] *= letThrough(sh.peak, y, e, w);
      else got.band[band][a] += e * y * w;
    }
  }
  for (let a = 0; a < NA; a++) {
    if (IS_LEVEL[a]) {
      let product = 1;
      for (let b = 0; b < n; b++) product *= bandLevel[b][a];
      got.level[a] *= product ** (1 / n);
      continue;
    }
    if (IS_PART[a]) continue;
    let sum = 0;
    let most = -Infinity;
    for (let b = 0; b < n; b++) {
      sum += got.band[b][a];
      most = Math.max(most, got.band[b][a]);
    }
    got.whole[a] += IS_MOMENT[a] ? most : sum / n;
  }
  const settings = s as unknown as Record<string, number>;
  for (const [a, k] of SCALED) {
    const v = settings[k];
    if (v === 1) continue;
    got.whole[a] *= v;
    for (const own of got.band) own[a] *= v;
  }

  // The chain's own modules.
  const clock = stepClock(st.clock, m.bps, m.playing, s.speed, L.pace, opts.still, step);
  outputs['clock.rate'] = clock;
  const trackInto = (a: Aspect) => {
    const [mod, knob] = ASPECT_KNOB[a];
    let sum = 0;
    for (const c of into.get(`${mod}|${knob}`) ?? []) if ((typeOf.get(c.from.module) ?? c.from.module) === 'track') sum += depthOf(c) * heightOf(shapeOf(c.value), pulse) * walkOf(c);
    return sum;
  };
  const aspects = stepChain(
    st.chain,
    m,
    s,
    { given: got, track: raw, bands, lit: env.level, envelope: env.out, glow: env.glow, clock, trackWalk: { look: trackInto('look'), behaviour: trackInto('behaviour') } },
    step,
    opts,
  );

  // Every other module's knobs, as the evaluator moves a knob.
  for (const x of p.modules) {
    if (knobs[x.id] || live.has(x.id)) continue;
    const t = types[x.type];
    if (!t) continue;
    if (t.knobs.some((knob) => COLUMN.has(`${x.id}|${knob.id}`))) continue;
    knobs[x.id] = knobsOf(x, t);
  }
  return { aspects, knobs, outputs, macros, moments };

  /** A module's knobs as its macros and cables move them. */
  function knobsOf(x: Module, t: ModuleType): Record<string, number> {
    const k: Record<string, number> = { ...x.knobs };
    for (const knob of t.knobs) {
      let base = finite(x.knobs[knob.id], knob.default);
      if (knob.kind !== 'step') for (const g of targets.get(`${x.id}|${knob.id}`) ?? []) base = g.from + (g.to - g.from) * macroOf(g.macro);
      k[knob.id] = moved(x.id, knob, base, into.get(`${x.id}|${knob.id}`) ?? []);
    }
    return k;
  }

  /** A knob moved from its base by its cables (evaluate.ts's rule: a swing adds, a level lets through; a band's cable
   *  into a knob not a band's own, the bands' mean and geometric mean). */
  function moved(module: string, k: Knob, base: number, cables: CableV2[]): number {
    if (!cables.length) return clamp(base, k.min, k.max);
    const bandSums = k.kind === 'step' ? new Float64Array(BANDS) : null;
    const ownBand = /\.\d+$/.test(k.id);
    let swing = 0;
    let level = 1;
    let bandSwing = 0;
    let bandLevel = 1;
    for (const c of cables) {
      const x = xOf(c);
      if (x === null) continue;
      const sh = shapeOf(c.value);
      const y = heightOf(sh, x);
      const w = walkOf(c);
      const e = depthOf(c);
      const apart = c.from.channel !== undefined && !ownBand;
      if (c.mode === 'level' && !bandSums) {
        if (apart) bandLevel *= letThrough(sh.peak, y, e, w);
        else level *= letThrough(sh.peak, y, e, w);
      } else if (apart) {
        bandSwing += e * y * w;
        const ch = c.from.channel!;
        if (bandSums && ch >= 0 && ch < BANDS) bandSums[ch] += e * y * w;
      } else swing += e * y * w;
    }
    if (bandSums) {
      // A step is not moved: what its cables give (the bands' largest) rising past a half asks for a moment.
      const key = `${module}|${k.id}`;
      const asked = swing + Math.max(...bandSums);
      const mo = (st.moments[key] ??= { armed: true, since: Infinity });
      mo.since += step;
      if (asked < 1 / 3) mo.armed = true;
      if (asked > 0.5 && mo.armed && mo.since >= MOMENT_GAP) {
        mo.armed = false;
        mo.since = 0;
        moments.push({ module, knob: k.id });
      }
      return clamp(base, k.min, k.max);
    }
    return clamp(base * level * bandLevel ** (1 / BANDS) + (swing + bandSwing / BANDS) * ((k.max - k.min) / 2), k.min, k.max);
  }
}
