// The one engine (decision 0057, step 2): every value and trigger of a v2 patch, a frame at a time, over its typed
// cables. Music, Track and the Envelope first (what every cable reads and what scales it); then the modulators, VCAs
// and Merges in the order their cables need (a cycle reads last frame's value, VCV Rack's rule); then what the cables
// give each of the Cratarr chain's columns, by the routes' rules (routes.ts give: a swing sums, a level lets through, a
// band's cable into a part's column is that part's own, into another the bands' mean, a moment's their largest; a
// column's scale is in its cables' amounts, 0057 §7e), a level column's gain input and a moment column's trigger; then
// the chain's own modules (chain.ts); and every other module's knobs as their cables move them (evaluate.ts's rule).
// What aspects.ts and evaluate.ts did between them, in one place and all of it wired.

import type { AspectsNow } from '../aspects';
import { MOMENT_GAP, heightOf, letThrough, shapeOf, type MomentRequest } from '../evaluate';
import { seedOf } from '../flame';
import type { Levers } from './sidechain';
import { modulatorOf, type ModulatorType } from '../modulators/modulators';
import type { Music } from '../music-core';
import { ASPECT_KNOB, type Knob, type Macro, type Module, type ModuleType, type Modulator, type SignalRef } from '../patch';
import { SHOW_PARTS, type CableV2, type PatchV2, type PortRef } from '../ports';
import { A, ASPECTS, B, BAND_SIGNALS, LEVEL_ASPECTS, MOMENT_ASPECTS, PART_ASPECTS, T, TRACK_SIGNALS, given, type Aspect, type Given } from '../routes';
import { BANDS, type VisualSettings } from '../settings.svelte';
import { newChain, stepChain, type ChainState } from './chain';
import { newDucker, stepDucker, type DuckerState } from './compressor';
import { FLASH_GUARD } from './sidechain';
import type { MeterValues } from '../meters';
import { NO_STRIKE, adsrOf, newClock, newEnvelope, newTrack, stepClock, stepEnvelope, stepTrack, type ClockState, type EnvelopeState, type TrackState } from './modules';

const NA = ASPECTS.length;
const IS_LEVEL = ASPECTS.map((a) => LEVEL_ASPECTS.includes(a));
const IS_PART = ASPECTS.map((a) => PART_ASPECTS.includes(a));
const IS_MOMENT = ASPECTS.map((a) => (MOMENT_ASPECTS as readonly Aspect[]).includes(a));
/** Each chain knob that is a column (`module|knob` → its place in ASPECTS); the Show's as its device flattened has
 *  them (`show/<part>|<knob>`, v2.ts), its Size the Flame's zoom. */
const COLUMN = new Map<string, number>();
for (const [a, [m, k]] of Object.entries(ASPECT_KNOB) as [Aspect, [string, string]][]) {
  COLUMN.set(`${m}|${k}`, A[a]);
  if (m !== 'show') continue;
  const part = SHOW_PARTS.find((x) => x.knob === k);
  if (k === 'size') COLUMN.set('flame|zoom', A[a]);
  else if (part) COLUMN.set(`show/${part.type}|${k}`, A[a]);
}
/** The chain's column a knob is (`module|knob`, flattened), by its place in ASPECTS. */
export const columnAt = (key: string): number | undefined => COLUMN.get(key);
/** The column a chain aspect's cables go into, flattened. */
const columnOf = (a: Aspect): string[] => [...COLUMN].filter(([, i]) => i === A[a]).map(([key]) => key);
/** Each ADSR gate's strikes, by the Music port its gate reads. */
const STRIKES: Record<string, 'hits' | 'drums'> = { hits: 'hits', percussiveHits: 'drums' };
/** The modules the graph works out channel by channel (0057 §7e): VCA, in × gain; Merge, a polyphonic input made one. */
const CHANNELS = BANDS;
const MERGE_MODES = ['sum', 'mean', 'max', 'geometric'] as const;
/** A knob's own inputs beside its value (ports.ts): a level knob's gain, a step knob's trigger to its next step. */
const GAIN = ':gain';
const NEXT = ':next';
/** A Meter's outputs (ports.ts METER, 0057 §3b's kinds), each the measure it is (meters.ts). */
const METER_OF: Record<string, (m: MeterValues) => number> = {
  motion: (m) => m.motion,
  shape: (m) => m.shapeChurn,
  colour: (m) => m.colourChurn,
  zoom: (m) => m.zoomPumping,
  looks: (m) => m.looksPerMinute,
};
/** Halved with reduced motion, as the routes read them. */
const HALVED = new Set(['beat', 'downbeat', 'section']);
const OPEN: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };
const BAND_PORTS: ReadonlySet<string> = new Set(BAND_SIGNALS);
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
  /** Each VCA's and Merge's channels last frame (a cycle through one reads them). */
  poly: Record<string, Float64Array>;
  /** Each trigger input's last value (`module|knob:next`): it fires as it rises past a half. */
  triggers: Record<string, number>;
  /** Each Ducker's state, and the Screen's flash guard's. */
  duckers: Record<string, DuckerState>;
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
    poly: {},
    triggers: {},
    duckers: {},
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
  /** What the Meter measured of the last frame drawn (meters.ts; the precheck's measure()): the sidechains' keys. */
  meters?: MeterValues;
};

/** One frame. `read`: the music's signals on 1 = typical (normal.ts signalsFor), for cables that read them so; `s`: the
 *  chain's knobs as settings (until the chain's modules hold their own). */
export function stepEngine(st: EngineState, p: PatchV2, types: Record<string, ModuleType>, m: Music, read: (ref: SignalRef) => number, s: VisualSettings, dt: number, opts: EngineOptions): EngineFrame {
  const L = opts.levers ?? OPEN;
  const step = Number.isFinite(dt) ? Math.max(0, dt) : 0;
  const typeOf = new Map(p.modules.map((x) => [x.id, x.type]));
  const moduleOf = new Map(p.modules.map((x) => [x.id, x]));
  const outputs: Record<string, number> = {};
  // Each ADSR's gate cable: the Music port it is keyed by, and how its strikes are read.
  const gateOf = new Map<string, CableV2>();
  for (const c of p.cables) if (typeOf.get(c.to.module) === 'adsr' && c.to.port === 'gate' && (typeOf.get(c.from.module) ?? c.from.module) === 'music') gateOf.set(c.to.module, c);

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
    return t && typeof t.create === 'function' ? (t as ModulatorType) : (modulatorOf(type) ?? null);
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
    // A VCA's or a Merge's output, as it is (a Merge's one channel whichever is asked); a Ducker's too.
    if (type === 'vca' || type === 'merge' || type === 'ducker') return channelsOf(f.module)[type === 'vca' ? (f.channel ?? 0) : 0];
    if (type === 'meter') return opts.meters ? finite(METER_OF[f.port]?.(opts.meters)) : 0;
    if (type === 'screen' && f.port === 'guard') return guard;
    if (type === 'adsr') {
      const gate = gateOf.get(f.module);
      // Its strikes read as its gate reads them: as they are (0..), or against the typical (1 = a typical one, halved
      // into 0..1 as a signal read so is); all its channels at once, none (as a band's signal read raw without its band).
      const out = adsrOut(f.module, f.channel);
      return gate?.from.reference === 'absolute' ? out : clamp(out / 2, 0, 1);
    }
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
  /** An ADSR's output on one channel now (its knobs `<stage>.<channel>`, `amount`): its gate's strikes, each one's
   *  velocity read as the gate cable reads (`absolute`: its strength; else against the typical, as normal.ts reads
   *  the signal). */
  function adsrOut(id: string, ch: number | undefined): number {
    const gate = gateOf.get(id);
    const kind = gate && STRIKES[gate.from.port];
    if (!gate || !kind || ch === undefined) return 0;
    const k = moduleOf.get(id)?.knobs ?? {};
    const strike = m.strikes?.[ch]?.[kind] ?? NO_STRIKE;
    const r = gate.from;
    const velocity = (v: number) => (r.reference === 'absolute' ? v : finite(read({ signal: r.port, band: ch, reference: r.reference, value: v })));
    const struck = { since: strike.since, velocity: velocity(strike.velocity), next: strike.next && { until: strike.next.until, velocity: velocity(strike.next.velocity) } };
    const v = adsrOf(struck, { attack: finite(k[`attack.${ch}`]), decay: finite(k[`decay.${ch}`]), sustain: finite(k[`sustain.${ch}`]), release: finite(k[`release.${ch}`], 0.15) });
    return v * finite(k.amount, 1);
  }
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
  // The Screen's flash guard (WCAG 2.3.1; claude-632's limiter on the output, step 3): fixed, keyed by the flashes the
  // last frame made, a tenth at most held back; no knob in it.
  const guard =
    p.modules.some((x) => x.type === 'screen') && opts.meters
      ? Math.max(0.1, stepDucker((st.duckers['screen|guard'] ??= newDucker()), 1, Math.max(opts.meters.flash, opts.meters.redFlash), FLASH_GUARD, step).gain)
      : 1;
  // The VCAs' and Merges' channels this frame (last frame's until each is worked out).
  const poly: Record<string, Float64Array> = {};
  const channelsOf = (id: string) => poly[id] ?? st.poly[id] ?? new Float64Array(CHANNELS);
  /** What a cable gives a port on channel `ch` (a knob's cable's rule: its curve at its source's x, times its depth
   *  and its walk); a VCA's or a Merge's source taken as it is, along its straight line where the value is a number. */
  const giveOf = (c: CableV2, ch: number): number => {
    const type = typeOf.get(c.from.module) ?? c.from.module;
    const sh = shapeOf(c.value);
    const along = c.from.channel === undefined && c.to.channel === undefined ? { ...c, from: { ...c.from, channel: ch } } : c;
    if (type === 'vca' || type === 'merge' || type === 'ducker') {
      const v = channelsOf(c.from.module)[type === 'vca' ? (c.from.channel ?? ch) : 0];
      return (sh.table ? heightOf(sh, clamp(v, 0, 1)) : sh.end * v) * depthOf(c) * walkOf(c);
    }
    const x = xOf(type === 'music' && BAND_PORTS.has(c.from.port) ? along : c);
    return x === null ? 0 : heightOf(sh, x) * depthOf(c) * walkOf(c);
  };
  /** What a port takes on channel `ch`: its cables' sum (a cable with no channel of its own reaches every channel). */
  const takes = (module: string, port: string, ch: number): number => {
    let sum = 0;
    for (const c of into.get(`${module}|${port}`) ?? []) if (c.to.channel === undefined || c.to.channel === ch) sum += giveOf(c, ch);
    return sum;
  };
  const plugged = (module: string, port: string, ch: number) => (into.get(`${module}|${port}`) ?? []).some((c) => c.to.channel === undefined || c.to.channel === ch);
  /** A VCA (in × gain, the gain its knob plus what its gain input takes, 0 to the knob's most; its input normalled to
   *  1) or a Merge (its channels' sum, mean, largest or geometric mean), channel by channel. */
  function process(x: Module) {
    const out = new Float64Array(CHANNELS);
    if (x.type === 'ducker') {
      // A Ducker (compressor.ts): its input (normalled to 1) held down while its key is over its threshold (the
      // threshold's gain input moving it, as the music's activity does a budget).
      const k = x.knobs;
      const knobs = {
        threshold: finite(k.threshold, 1) * gainOf(x.id, 'threshold'),
        depth: finite(k.depth, 0),
        attack: finite(k.attack, 0),
        hold: finite(k.hold, 0),
        release: finite(k.release, 1),
        hysteresis: finite(k.hysteresis, 1),
      };
      const input = plugged(x.id, 'in', 0) ? takes(x.id, 'in', 0) : 1;
      out.fill(stepDucker((st.duckers[x.id] ??= newDucker()), input, takes(x.id, 'key', 0), knobs, step).out);
      poly[x.id] = out;
      return;
    }
    if (x.type === 'vca') {
      const gain = finite(x.knobs.gain, 1);
      for (let ch = 0; ch < CHANNELS; ch++) out[ch] = (plugged(x.id, 'in', ch) ? takes(x.id, 'in', ch) : 1) * clamp(gain + takes(x.id, 'cv', ch), 0, 2);
    } else {
      const v = Array.from({ length: CHANNELS }, (_, ch) => takes(x.id, 'in', ch));
      const mode = MERGE_MODES[clamp(Math.round(finite(x.knobs.mode, 1)), 0, MERGE_MODES.length - 1)];
      const one =
        mode === 'sum' ? v.reduce((a, b) => a + b, 0) : mode === 'mean' ? v.reduce((a, b) => a + b, 0) / CHANNELS : mode === 'max' ? Math.max(...v) : v.reduce((a, b) => a * b, 1) ** (1 / CHANNELS);
      out.fill(one);
    }
    poly[x.id] = out;
  }
  /** A level knob's gain input (normalled to 1), 0..1. */
  const gainOf = (module: string, knob: string) => (into.has(`${module}|${knob}${GAIN}`) ? clamp(takes(module, `${knob}${GAIN}`, 0), 0, 1) : 1);
  /** A step knob's trigger input: whether it rose past a half this frame (null: nothing plugged into it). */
  const fired = (module: string, knob: string): boolean | null => {
    const key = `${module}|${knob}${NEXT}`;
    if (!into.has(key)) return null;
    const v = takes(module, `${knob}${NEXT}`, 0);
    const was = finite(st.triggers[key], 0);
    st.triggers[key] = v;
    return v > 0.5 && was <= 0.5;
  };

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
      if (type === 'music' || type === 'adsr') {
        const v = type === 'adsr' ? adsrOut(c.from.module, c.from.channel) : c.from.reference === 'absolute' ? (xOf(c) ?? 0) : finite(read({ signal: c.from.port, band: c.from.channel, reference: c.from.reference, speed: c.from.speed }));
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
  const processors = p.modules.filter((x) => x.type === 'vca' || x.type === 'merge' || x.type === 'ducker');
  const nodes: Node[] = [...p.macros.map((macro) => ({ id: macro.id, macro })), ...running.map((module) => ({ id: module.id, module })), ...processors.map((module) => ({ id: module.id, module }))];
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
    if (x.type === 'vca' || x.type === 'merge' || x.type === 'ducker') {
      process(x);
      continue;
    }
    const t = modulatorType(x.type)!;
    const k = knobsOf(x, t);
    for (const port of t.inputs ?? []) k[port] = portSum(x.id, port);
    knobs[x.id] = k;
    outputs[x.id] = st.modulators[x.id].run.step(step, read, k);
  }
  for (const id of Object.keys(st.modulators)) if (id in outputs) st.values[id] = outputs[id];
  st.poly = { ...poly };
  for (const [id, v] of Object.entries(poly)) outputs[`${id}.out`] = v[0];
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
  // A level column's gain input (its VCAs), a moment column's trigger (its Gate): 0057 §7e.
  const firedNow: Partial<Record<Aspect, boolean>> = {};
  for (const key of into.keys()) {
    const gain = key.endsWith(GAIN);
    if (!gain && !key.endsWith(NEXT)) continue;
    const base = key.slice(0, -(gain ? GAIN : NEXT).length);
    const a = COLUMN.get(base);
    if (a === undefined) continue;
    const [module, knob] = base.split('|');
    if (gain && IS_LEVEL[a]) got.level[a] *= gainOf(module, knob);
    // A swing column's gain scales its modulation (Bitwig's modulation scaling): its cables, a part's own among them.
    if (gain && !IS_LEVEL[a] && !IS_MOMENT[a]) {
      const g = gainOf(module, knob);
      got.whole[a] *= g;
      for (const own of got.band) own[a] *= g;
    }
    if (!gain && IS_MOMENT[a]) firedNow[ASPECTS[a]] ||= fired(module, knob) ?? false;
  }

  // The chain's own modules, each as its gain inputs hold it (the sidechains' Duckers, 0057 §7e): the Clock's speed
  // (its pace), the Accent's accent (the flash guard: its swing, and its courses slower), the Flame's evolving times
  // (a time's gain stretches it). Levers given (today's sidechaining, until the patch has its Duckers) act with them.
  const held: Levers = { ...L, pace: L.pace * gainOf('clock', 'speed'), swing: L.swing * gainOf('accent', 'accent'), evolution: L.evolution / Math.max(0.01, gainOf('flame', 'evolution')) };
  const clock = stepClock(st.clock, m.bps, m.playing, s.speed, held.pace, opts.still, step);
  outputs['clock.rate'] = clock;
  const trackInto = (a: Aspect) => {
    let sum = 0;
    for (const c of columnOf(a).flatMap((key) => into.get(key) ?? [])) if ((typeOf.get(c.from.module) ?? c.from.module) === 'track') sum += depthOf(c) * heightOf(shapeOf(c.value), pulse) * walkOf(c);
    return sum;
  };
  const aspects = stepChain(
    st.chain,
    m,
    s,
    { given: got, track: raw, bands, lit: env.level, envelope: env.out, glow: env.glow, clock, trackWalk: { look: trackInto('look'), behaviour: trackInto('behaviour') }, fired: firedNow },
    step,
    { ...opts, levers: held },
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
    if (k.kind === 'step' && fired(module, k.id)) moments.push({ module, knob: k.id });
    else if (k.kind !== 'step') base = k.time ? base / Math.max(0.01, gainOf(module, k.id)) : base * gainOf(module, k.id);
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
