// The one engine (decision 0057, step 2): every value and trigger of a v2 patch, a frame at a time, over its typed
// cables. Music, Track and the Envelope first (what every cable reads and what scales it); then the modulators, VCAs
// and Merges in the order their cables need (a cycle reads last frame's value, VCV Rack's rule); then what the cables
// give each of the Cratarr device's columns, by the routes' rules (routes.ts give: a swing sums, a level lets through, a
// band's cable into a part's column is that part's own, into another the bands' mean, a moment's their largest; a
// column's scale is in its cables' amounts, 0057 §7e), a level column's gain input and a moment column's trigger; then
// the chain's own modules (chain.ts); and every other module's knobs as their cables move them (evaluate.ts's rule).
// What aspects.ts and evaluate.ts did between them, in one place and all of it wired.

import type { AspectsNow } from '../aspects';
import { heightOf, shapeOf, type Shape } from '../evaluate';
import { seedOf } from '../flame';
import type { Levers } from './sidechain';
import { modulatorOf, type ModulatorType } from '../modulators/modulators';
import type { Music } from '../music-core';
import { ASPECT_KNOB, type Knob, type Macro, type Module, type ModuleType, type Modulator, type SignalRef } from '../patch';
import { COLUMN_OFFSETS, ENGINE_MODULES, RANDOMIZE_SCOPES, READINGS, SHOW_PARTS, type CableV2, type PatchV2, type PortRef } from '../ports';
import { A, ASPECTS, B, BAND_SIGNALS, LEVEL_ASPECTS, MOMENT_ASPECTS, PART_ASPECTS, T, TRACK_SIGNALS, given, type Aspect, type Given } from '../routes';
import { BANDS, type VisualSettings } from '../settings.svelte';
import { newChain, stepChain, type ChainState } from './chain';
import { newDucker, stepDucker, type DuckerState } from './compressor';
import { newClockTriggers, stepClockTriggers, type ClockTriggerState, type ClockTriggers } from './clock';
import { gridOf, type Grid, type GridPoint } from './grid';
import { FLASH_GUARD } from './sidechain';
import type { MeterValues } from '../meters';
import { slotKnobOf, slotOf } from './effect-slots';
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
const COLUMNS_OF = new Map<number, string[]>();
for (const [key, i] of COLUMN) (COLUMNS_OF.get(i) ?? COLUMNS_OF.set(i, []).get(i)!).push(key);
const columnOf = (a: Aspect): string[] => COLUMNS_OF.get(A[a]) ?? [];
/** Each ADSR gate's strikes, by the Music port its gate reads. */
const STRIKES: Record<string, 'hits' | 'drums'> = { hits: 'hits', percussiveHits: 'drums' };
/** The modules the graph works out channel by channel (0057 §7e): VCA, in × gain; Merge, a polyphonic input made one. */
const CHANNELS = BANDS;
const MERGE_MODES = ['sum', 'mean', 'max', 'geometric'] as const;
/** The engine's own module types (ports.ts), for their knobs' tapers where the caller's types do not have them. */
const ENGINE_TYPES: Record<string, ModuleType> = Object.fromEntries(ENGINE_MODULES.map((t) => [t.type, t]));
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
  /** Each VCA's and Merge's channels last frame (a cycle through one reads them). */
  poly: Record<string, Float64Array>;
  /** Each trigger input's last value (`module|knob:next`): it fires as it rises past a half. */
  triggers: Record<string, number>;
  /** Each Ducker's state, and the Screen's flash guard's. */
  duckers: Record<string, DuckerState>;
  /** The frame's outputs, one object while the patch is the same (its keys the same each frame), and the plan it is for. */
  outputs: Record<string, number>;
  outputsFor: unknown;
  /** The Clock's beat, bar and phrase triggers (clock.ts). */
  clockTriggers: ClockTriggerState;
  /** Each Grid's points and channels, and the knobs they were laid out for (laid out again only when those change). */
  grids: Record<string, { key: string; grid: Grid }>;
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
    poly: {},
    triggers: {},
    duckers: {},
    outputs: {},
    outputsFor: null,
    clockTriggers: newClockTriggers(),
    grids: {},
  };
}

/** A step knob to move on (its module flattened). */
export type NextRequest = { module: string; knob: string };

export type EngineFrame = {
  /** What the picture is drawn with this frame (today's renderer's input, until step 3's modules take knob values); null
   *  where the patch has no Cratarr device (a blank slate: nothing is drawn). */
  aspects: AspectsNow | null;
  /** Every module's knobs as their cables move them (the chain's: its settings, its columns given in `aspects`). */
  knobs: Record<string, Record<string, number>>;
  /** Every value output this frame, by `module.port`: the same object each frame while the patch is the same (copy what
   *  is to be kept). */
  outputs: Record<string, number>;
  /** Each macro's value this frame. */
  macros: Record<string, number>;
  /** The steps past the chain whose cables ask for a moment (morph.ts takes it at the module's neutral). */
  /** The steps whose Next a trigger fired this frame (0057 §7e): each to move on to its next step. */
  nexts: NextRequest[];
  /** Each Grid's points (its `points` output, for Instance: claude-632's compositor). */
  points: Record<string, GridPoint[]>;
  /** What reaches each points input (`module.port`: a Fluid's `at`, where it pushes): what is cabled into it (a Flame's
   *  points, a Grid's), else what it is normalled to (the Flame's points). For the compositor, which runs the sim. */
  pointsAt: Record<string, readonly (Point | null)[]>;
  /** The Dice whose trigger rose this frame (0057 §7f): each one's seed and scope, for the page to randomize the patch
   *  with (randomize.ts: over frames, as one undoable change through the store). */
  dice: DiceRoll[];
};

/** A Dice's roll: which, the seed its seed input gives (Music's `seed`: the track's own), its scope. */
export type DiceRoll = { module: string; seed: number; scope: (typeof RANDOMIZE_SCOPES)[number] };

export type EngineOptions = {
  still: boolean;
  now: number;
  fluidFull: number;
  levers?: Levers;
  /** Each walking cable's factor now, by its id (1: as set). */
  walk?: Record<string, number>;
  /** What the Meter measured of the last frame drawn (meters.ts; the precheck's measure()): the sidechains' keys. */
  meters?: MeterValues;
  /** The Flame's parts where they are on the screen (0..1, y up; null: a part not drawn), as the page measures them: it
   *  holds the drawn flame and its framing. The Flame's `points` output (0057, the modular Fluid). */
  flamePoints?: readonly (Point | null)[];
};

/** A point on the screen, 0..1 each way. */
export type Point = readonly [number, number];

/** A modulator's type: the caller's, else modulators.ts's (a renamed one's too). */
function modulatorTypeIn(types: Record<string, ModuleType>, type: string): ModulatorType | null {
  const t = types[type] as Partial<ModulatorType> | undefined;
  return t && typeof t.create === 'function' ? (t as ModulatorType) : (modulatorOf(type) ?? null);
}

type Node = { id: string; macro?: Macro; module?: Module };
/** What a patch's structure gives the engine, worked out once a patch (a patch is not changed in place: a changed one is
 *  a new object, as the store makes it; its modules' knobs are read live). */
type Plan = {
  types: Record<string, ModuleType>;
  typeOf: Map<string, string>;
  moduleOf: Map<string, Module>;
  /** The first module of each type (the engine's own are one a patch). */
  first: Map<string, Module>;
  flame: Module | undefined;
  /** The chain's path (beside its Flame). */
  at: string;
  gateOf: Map<string, CableV2>;
  /** Each port's cables (`module|port`). */
  into: Map<string, CableV2[]>;
  running: Module[];
  /** The macros, modulators, VCAs, Merges and Duckers in the order their cables need. */
  order: Node[];
  macroIds: Set<string>;
  targets: Map<string, { macro: Macro; from: number; to: number }[]>;
  modulesNow: Set<string>;
  grids: Module[];
  /** Each port's cables as a knob takes them (a polyphonic one as one a channel), the same objects each frame. */
  expanded: WeakMap<readonly CableV2[], CableV2[]>;
  dice: Module[];
  fluids: Module[];
  /** Each Shaper's curve as a shape, and the knobs it was made from. */
  shapers: Map<string, { key: string; shape: Shape }>;
  /** Each key's (`module|port`) chain column, where it is one. */
  columns: Map<string, number | undefined>;
  /** Each module's ports' cables reaching each channel (a channel-less one from a band's signal as that channel's). */
  ports: Map<string, Map<string, Entry[][]>>;
  /** Each module's knobs' gain inputs and next triggers, by the knob. */
  gains: Map<string, Map<string, Entry[][]>>;
  nexts: Map<string, Map<string, Entry[][]>>;
  /** Each cable's shape, as its value was when last asked. */
  shapes: WeakMap<CableV2, { v: CableV2['value']; shape: Shape }>;
  /** The chain's columns and the cables into each, as a knob takes them, each with its source's type and its shape
   *  (worked out at the first frame); the columns a band's cable reaches. */
  columnCables: { a: number; cables: { c: CableV2; type: string; v: CableV2['value']; shape: Shape }[] }[] | null;
  bandColumns: number[];
  /** The modulators' ids; each Music row's reading, and the knob value it was read at. */
  live: Set<string>;
  readings: Map<string, { at: unknown; knob: string; reading: Reading }>;
  processorIds: string[];
  outKeys: string[];
  /** Worked out at the first frame (they need the chain's place): the columns' gain inputs and triggers, the columns'
   *  resting values, the cables from the Track into the walks, the modules with a column among their knobs. */
  columnInputs: { a: number; module: string; knob: string; gain: boolean }[] | null;
  columnOffsets: { a: number; rel: string; knob: string }[] | null;
  trackCables: Partial<Record<Aspect, CableV2[]>>;
  /** Each module's ports' cables, by module then port; the modules past the chain whose knobs the engine moves
   *  (worked out at the first frame). */
  intoOf: Map<string, Map<string, CableV2[]>>;
  others: { x: Module; t: ModuleType }[] | null;
  /** Each module's knobs' macro targets, by module then knob. */
  targetsOf: Map<string, Map<string, { macro: Macro; from: number; to: number }[]>>;
  /** The VCAs whose channels differ (something polyphonic reaches them); the rest are worked out once, a channel. */
  polyphonic: Set<string>;
};
const plans = new WeakMap<PatchV2, Plan>();
const PROCESSORS = new Set(['vca', 'merge', 'ducker', 'shaper']);
/** The processors whose output has channels of their own (a Merge's and a Ducker's are one). */
const CHANNELLED = new Set(['vca', 'shaper']);

/** An ADSR's channel's knobs' names, and what a call reads its strike with (made once, not a call). */
const ATTACK_AT = Array.from({ length: 64 }, (_, ch) => `attack.${ch}`);
const DECAY_AT = Array.from({ length: 64 }, (_, ch) => `decay.${ch}`);
const SUSTAIN_AT = Array.from({ length: 64 }, (_, ch) => `sustain.${ch}`);
const RELEASE_AT = Array.from({ length: 64 }, (_, ch) => `release.${ch}`);
const CONTINUITY_AT = Array.from({ length: 64 }, (_, k) => `continuity.${k}`);
const EVOLUTION_AT = Array.from({ length: 64 }, (_, k) => `evolution.${k}`);
const NO_CABLES: CableV2[] = [];
const STRUCK_NEXT = { until: 0, velocity: 0 };
const STRUCK: { since: number; velocity: number; next: { until: number; velocity: number } | null } = { since: Infinity, velocity: 0, next: null };
const ADSR_KNOBS = { attack: 0, decay: 0, sustain: 0, release: 0.15, retrigger: 0 };
/** No channels yet (a VCA, Merge or Ducker not yet worked out), and a scratch for working one out. */
const NO_CHANNELS = new Float64Array(64);
const SCRATCH = new Float64Array(6);
/** A new track's strike (all of it, as it starts), for its ADSR. */
const TRACK_STRIKE = { since: Infinity, velocity: 1, next: null };

/** A cable as the hot loops take it: its source's type, and its shape as its value was when last asked. */
type Entry = { c: CableV2; type: string; v: CableV2['value']; shape: Shape };
const entryOf = (c: CableV2, typeOf: Map<string, string>): Entry => ({ c, type: typeOf.get(c.from.module) ?? c.from.module, v: c.value, shape: shapeOf(c.value) });

/** How a Music row is read: its signal and its reading. */
type Reading = { signal: string; reference?: (typeof READINGS)[number] };

/** The cables into each module's ports (those whose name ends in `suffix`, by the name before it), reaching each
 *  channel; a channel-less cable from a band's signal as one a channel, each its band's (VCV's polyphonic cable). */
function byPort(into: Map<string, CableV2[]>, typeOf: Map<string, string>, suffix: string): Map<string, Map<string, Entry[][]>> {
  const out = new Map<string, Map<string, Entry[][]>>();
  for (const [key, cs] of into) {
    const bar = key.indexOf('|');
    const module = key.slice(0, bar);
    const port = key.slice(bar + 1);
    if (suffix ? !port.endsWith(suffix) : port.endsWith(GAIN) || port.endsWith(NEXT)) continue;
    const name = suffix ? port.slice(0, -suffix.length) : port;
    const reaching = Array.from({ length: CHANNELS }, (_, ch) =>
      cs
        .filter((c) => c.to.channel === undefined || c.to.channel === ch)
        .map((c) => {
          const band = (typeOf.get(c.from.module) ?? c.from.module) === 'music' && BAND_PORTS.has(c.from.port);
          return entryOf(band && c.from.channel === undefined && c.to.channel === undefined ? { ...c, from: { ...c.from, channel: ch } } : c, typeOf);
        }),
    );
    (out.get(module) ?? out.set(module, new Map()).get(module)!).set(name, reaching);
  }
  return out;
}

/** An ADSR keyed by a new track (Music's `newtrack`, 0057 §7f: what the Track's pulse was): one channel, not a band's. */
function trackAdsr(id: string, typeOf: Map<string, string>, into: Map<string, CableV2[]>): boolean {
  return (into.get(`${id}|gate`) ?? []).some((c) => (typeOf.get(c.from.module) ?? c.from.module) === 'music' && c.from.port === 'newtrack');
}

/** The VCAs something polyphonic reaches: a cable onto one channel, from a band's signal or an ADSR without one, or
 *  from a polyphonic VCA. */
function polyphonicOf(p: PatchV2, typeOf: Map<string, string>, into: Map<string, CableV2[]>): Set<string> {
  const out = new Set<string>();
  const vcas = p.modules.filter((x) => x.type === 'vca');
  for (let changed = true; changed; ) {
    changed = false;
    for (const x of vcas) {
      if (out.has(x.id)) continue;
      const cs = [...(into.get(`${x.id}|in`) ?? []), ...(into.get(`${x.id}|cv`) ?? [])];
      const poly = cs.some((c) => {
        if (c.to.channel !== undefined) return true;
        if (c.from.channel !== undefined) return false;
        const type = typeOf.get(c.from.module) ?? c.from.module;
        return (type === 'music' && BAND_PORTS.has(c.from.port)) || (type === 'adsr' && !trackAdsr(c.from.module, typeOf, into)) || type === 'shaper' || (type === 'vca' && out.has(c.from.module));
      });
      if (poly) {
        out.add(x.id);
        changed = true;
      }
    }
  }
  return out;
}

/** The same module types (a caller's map may be a new object each frame). */
function sameTypes(a: Record<string, ModuleType>, b: Record<string, ModuleType>): boolean {
  if (a === b) return true;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => a[k] === b[k]);
}

function planOf(p: PatchV2, types: Record<string, ModuleType>): Plan {
  const kept = plans.get(p);
  if (kept && sameTypes(kept.types, types)) return kept;
  const typeOf = new Map(p.modules.map((x) => [x.id, x.type]));
  const moduleOf = new Map(p.modules.map((x) => [x.id, x]));
  const first = new Map<string, Module>();
  for (const x of p.modules) if (!first.has(x.type)) first.set(x.type, x);
  // The Loudness: the first Envelope Follower on Music's loudness (its input uncabled, at its normal), stepped by the
  // engine itself; every other runs as a modulator.
  const loudness = p.modules.find((x) => x.type === 'envelope' && !p.cables.some((c) => c.to.module === x.id && c.to.port === 'in'));
  if (loudness) first.set('envelope', loudness);
  else first.delete('envelope');
  const flame = first.get('flame');
  const gateOf = new Map<string, CableV2>();
  for (const c of p.cables) if (typeOf.get(c.to.module) === 'adsr' && c.to.port === 'gate' && (typeOf.get(c.from.module) ?? c.from.module) === 'music') gateOf.set(c.to.module, c);
  const into = new Map<string, CableV2[]>();
  for (const c of p.cables) {
    const key = `${c.to.module}|${c.to.port}`;
    (into.get(key) ?? into.set(key, []).get(key)!).push(c);
  }
  const running = p.modules.filter((x) => x !== loudness && modulatorTypeIn(types, x.type));
  const macroIds = new Set(p.macros.map((x) => x.id));
  const targets = new Map<string, { macro: Macro; from: number; to: number }[]>();
  for (const macro of p.macros)
    for (const t of macro.targets) {
      const key = `${t.module}|${t.knob}`;
      (targets.get(key) ?? targets.set(key, []).get(key)!).push({ macro, from: t.from, to: t.to });
    }
  // The order: each node after what its cables (and a modulator's macros) need; a cycle reads last frame's (VCV's rule).
  const nodes: Node[] = [...p.macros.map((macro) => ({ id: macro.id, macro })), ...running.map((module) => ({ id: module.id, module })), ...p.modules.filter((x) => PROCESSORS.has(x.type)).map((module) => ({ id: module.id, module }))];
  const ids = new Set(nodes.map((x) => x.id));
  const needs = new Map<string, Set<string>>(nodes.map((x) => [x.id, new Set<string>()]));
  for (const c of p.cables) if (ids.has(c.from.module) && ids.has(c.to.module) && c.from.module !== c.to.module) needs.get(c.to.module)!.add(c.from.module);
  for (const macro of p.macros) for (const t of macro.targets) if (ids.has(t.module) && !macroIds.has(t.module)) needs.get(t.module)!.add(macro.id);
  const done = new Set<string>();
  const order: Node[] = [];
  while (done.size < nodes.length) {
    const next = nodes.find((x) => !done.has(x.id) && [...needs.get(x.id)!].every((d) => done.has(d))) ?? nodes.find((x) => !done.has(x.id))!;
    done.add(next.id);
    order.push(next);
  }
  const plan: Plan = {
    types,
    typeOf,
    moduleOf,
    first,
    flame,
    at: flame ? flame.id.slice(0, flame.id.lastIndexOf('/') + 1) : '',
    gateOf,
    into,
    running,
    order,
    macroIds,
    targets,
    modulesNow: new Set(p.modules.map((x) => x.id)),
    grids: p.modules.filter((x) => x.type === 'grid'),
    dice: p.modules.filter((x) => x.type === 'dice'),
    fluids: p.modules.filter((x) => x.type === 'fluid'),
    shapers: new Map(),
    expanded: new WeakMap(),
    columns: new Map(),
    polyphonic: polyphonicOf(p, typeOf, into),
    ports: byPort(into, typeOf, ''),
    gains: byPort(into, typeOf, GAIN),
    nexts: byPort(into, typeOf, NEXT),
    shapes: new WeakMap(),
    columnCables: null,
    bandColumns: [],
    live: new Set(running.map((x) => x.id)),
    readings: new Map(),
    processorIds: p.modules.filter((x) => PROCESSORS.has(x.type)).map((x) => x.id),
    outKeys: p.modules.filter((x) => PROCESSORS.has(x.type)).map((x) => `${x.id}.out`),
    columnInputs: null,
    columnOffsets: null,
    intoOf: (() => {
      const out = new Map<string, Map<string, CableV2[]>>();
      for (const c of p.cables) {
        const ports = out.get(c.to.module) ?? out.set(c.to.module, new Map()).get(c.to.module)!;
        (ports.get(c.to.port) ?? ports.set(c.to.port, []).get(c.to.port)!).push(c);
      }
      return out;
    })(),
    others: null,
    trackCables: {},
    targetsOf: (() => {
      const out = new Map<string, Map<string, { macro: Macro; from: number; to: number }[]>>();
      for (const macro of p.macros)
        for (const t of macro.targets) {
          const knobs = out.get(t.module) ?? out.set(t.module, new Map()).get(t.module)!;
          (knobs.get(t.knob) ?? knobs.set(t.knob, []).get(t.knob)!).push({ macro, from: t.from, to: t.to });
        }
      return out;
    })(),
  };
  plans.set(p, plan);
  return plan;
}

/** One frame. `read`: the music's signals on 1 = typical (normal.ts signalsFor), for cables that read them so; `s`: the
 *  chain's knobs as settings (until the chain's modules hold their own). */
export function stepEngine(st: EngineState, p: PatchV2, types: Record<string, ModuleType>, m: Music, read: (ref: SignalRef) => number, s: VisualSettings, dt: number, opts: EngineOptions): EngineFrame {
  const L = opts.levers ?? OPEN;
  const step = Number.isFinite(dt) ? Math.max(0, dt) : 0;
  const plan = planOf(p, types);
  const { typeOf, moduleOf, into, macroIds, targets } = plan;
  if (st.outputsFor !== plan) {
    st.outputs = {};
    st.outputsFor = plan;
  }
  // Each key is written again every frame; one read before its module this frame is last frame's (a cycle's rule).
  const outputs = st.outputs;
  // The Cratarr device (0057 §3; the owner: a device, "Cratarr"), where the patch has it: found by its Flame, its other
  // modules beside it (the Flame's path: the top level, or inside the device as flatten names them). None: a blank
  // slate, nothing drawn.
  const flameModule = plan.flame;
  const at = plan.at;
  const ofType = (type: string) => plan.first.get(type);
  const inChain = (rel: string) => moduleOf.get(at + rel);
  const knobOf = (x: Module | undefined, knob: string, or: number) => finite(x?.knobs[knob], or);
  /** A key (`module|knob`, flattened) as the chain's column it is. */
  const colAt = (key: string) => {
    if (plan.columns.has(key)) return plan.columns.get(key);
    const a = key.startsWith(at) ? COLUMN.get(key.slice(at.length)) : effectColumn(key);
    plan.columns.set(key, a);
    return a;
  };
  /** An effect module's knob (0057 §3, wherever it sits) as its slot's column: `fx-rotate|amount` Effect 1's Amount. */
  function effectColumn(key: string): number | undefined {
    const bar = key.indexOf('|');
    const id = key.slice(0, bar);
    const type = typeOf.get(id);
    const slot = type ? slotOf(id, type) : null;
    const knob = slot && slotKnobOf(type!, key.slice(bar + 1));
    return slot && knob ? COLUMN.get(`${slot}|${knob}`) : undefined;
  }
  /** The chain's settings from its modules' knobs (the patch is the truth, 0057 Phase B); a knob a patch has no module
   *  for, as the settings had it. */
  // Intensity, Flash and Speed are the rack's macros on these knobs (0057: the Whole frame gone): as last frame left them.
  const macroedKnob = (x: Module | undefined, knob: string, or: number) => {
    let base = knobOf(x, knob, or);
    for (const g of (x && plan.targetsOf.get(x.id)?.get(knob)) || []) base = g.from + (g.to - g.from) * finite(st.macros[g.macro.id], clamp(finite(g.macro.value), 0, 1));
    return base;
  };
  const chainSettings = {
    lighting: s.lighting,
    flash: macroedKnob(ofType('accent'), 'accent', s.flash),
    speed: macroedKnob(ofType('clock'), 'speed', s.speed),
    intensity: macroedKnob(ofType('envelope'), 'intensity', s.intensity),
    continuity: s.continuity.map((v, k) => knobOf(flameModule, CONTINUITY_AT[k], v)),
    evolution: s.evolution.map((v, k) => knobOf(flameModule, EVOLUTION_AT[k], v)),
    fluid: knobOf(inChain('fluid'), 'fluid', s.fluid),
    facets: knobOf(inChain('fluid'), 'facets', s.facets),
    tunnel: knobOf(inChain('tunnel'), 'tunnel', s.tunnel),
    dive: knobOf(inChain('dive'), 'dive', s.dive),
    // Effects: the rack's macro on the effects' Amounts (folded into them: 1) where the Whole frame is gone.
    effects: knobOf(inChain('frame'), 'effects', inChain('frame') ? s.effects : 1),
    smoothing: knobOf(inChain('show/blur'), 'smoothing', s.smoothing),
    haze: knobOf(inChain('show/curve'), 'haze', s.haze),
  } as VisualSettings;
  // How a Music row is read (its `reference.<signal>` knob: 0057 §7e, a cable no longer says); unset, as normal.ts
  // reads it by default.
  const musicModule = ofType('music');
  const readingOf = (port: string): Reading => {
    const kept = plan.readings.get(port);
    if (kept && kept.at === musicModule?.knobs[kept.knob]) return kept.reading;
    const reading = readingNow(port);
    const knob = `reference.${reading.signal}`;
    plan.readings.set(port, { at: musicModule?.knobs[knob], knob, reading });
    return reading;
  };
  const readingNow = (port: string): Reading => {
    const r = musicModule?.knobs[`reference.${port}`];
    const reference = typeof r === 'number' && Number.isFinite(r) ? READINGS[clamp(Math.round(r), 0, READINGS.length - 1)] : undefined;
    return { signal: port, ...(reference ? { reference } : {}) };
  };
  /** How an ADSR reads its strikes (its `reference` knob; absolute where unset, as the routes read the hits). */
  const adsrReading = (id: string) => READINGS[clamp(Math.round(finite(moduleOf.get(id)?.knobs.reference, 2)), 0, READINGS.length - 1)];
  // Each ADSR's gate cable: the Music port it is keyed by, and how its strikes are read.
  const gateOf = plan.gateOf;

  // Music's raw signals, the Envelope, the Track's pulse.
  const loud = ofType('envelope');
  const loudId = loud?.id ?? 'envelope';
  const env = stepEnvelope(st.envelope, m.intensity, m.playing, chainSettings.intensity, step, knobOf(loud, 'attack', 0.2), knobOf(loud, 'release', 0.2));
  const raw = st.signals;
  raw.set(m.track);
  raw[T.loudness] = env.level;
  if (opts.still) for (const t of [T.beat, T.downbeat, T.section]) raw[t] *= 0.5;
  // A new track (0057 §7f, Music's `newtrack`: a trigger the frame it starts, the visuals opening on one too) and its
  // seed (Music's `seed`: the same for the same track, 0..1, as flame.ts seedOf has it).
  const newTrack = m.key !== st.track.key ? 1 : 0;
  const pulse = stepTrack(st.track, m.key, opts.now, m.bps);
  const trackSeed = seedOf(m.key ?? 'cratarr') / 4294967296;
  raw[T.track] = pulse;
  const bands = m.bands ?? st.standIn;
  outputs[`${loudId}.level`] = env.level;
  outputs[`${loudId}.out`] = env.out;
  outputs[`${loudId}.glow`] = env.glow;
  const index = m.beat?.index ?? null;
  const onBeat = index !== null && st.beat !== undefined && index !== st.beat ? 1 : 0;
  st.beat = index;
  // The Clock's triggers (its beat, bar and phrase, each by its ratio), where the patch has a Clock.
  const clockModule = ofType('clock');
  const ticks: ClockTriggers = clockModule ? stepClockTriggers(st.clockTriggers, m, clockModule.knobs, step) : { beat: 0, bar: 0, phrase: 0 };
  if (clockModule) for (const k of ['beat', 'bar', 'phrase'] as const) outputs[`${clockModule.id}.${k}`] = ticks[k];
  // Each Grid laid out by its knobs as set (seeded by its id: the same every frame and on every device).
  const points: Record<string, GridPoint[]> = {};
  for (const x of plan.grids) {
    const key = JSON.stringify(x.knobs);
    if (st.grids[x.id]?.key !== key) st.grids[x.id] = { key, grid: gridOf(x.knobs, seedOf(x.id)) };
    points[x.id] = st.grids[x.id].grid.points;
  }
  for (const id of Object.keys(st.grids)) if (!(id in points)) delete st.grids[id];

  // The modulators running: one each, anew where its type changed, none for those gone.
  const modulatorType = (type: string) => modulatorTypeIn(types, type);
  const running = plan.running;
  const live = plan.live;
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
  const xOf = (c: CableV2, known?: string): number | null => {
    const f = c.from;
    const type = known ?? typeOf.get(f.module) ?? f.module;
    // A VCA's or a Merge's output, as it is (a Merge's one channel whichever is asked); a Ducker's too.
    if (PROCESSORS.has(type)) return channelsOf(f.module)[CHANNELLED.has(type) ? (f.channel ?? 0) : 0];
    if (type === 'meter') return opts.meters ? finite(METER_OF[f.port]?.(opts.meters)) : 0;
    if (type === 'screen' && f.port === 'guard') return guard;
    if (type === 'adsr') {
      const gate = gateOf.get(f.module);
      // Its strikes read as its gate reads them: as they are (0..), or against the typical (1 = a typical one, halved
      // into 0..1 as a signal read so is); all its channels at once, none (as a band's signal read raw without its band).
      const out = adsrOut(f.module, f.channel);
      return !gate || adsrReading(f.module) === 'absolute' ? out : clamp(out / 2, 0, 1);
    }
    if (type === 'music') {
      if (f.port === 'playing') return m.playing ? 1 : 0;
      if (f.port === 'newtrack') return newTrack;
      if (f.port === 'seed') return trackSeed;
      const r = readingOf(f.port);
      if (r.reference === 'absolute') {
        const t = T[r.signal as keyof typeof T];
        if (f.channel === undefined && t !== undefined) return raw[t];
        const b = B[r.signal as keyof typeof B];
        return b !== undefined && f.channel !== undefined ? (bands[f.channel]?.[b] ?? 0) * (opts.still && HALVED.has(r.signal) ? 0.5 : 1) : 0;
      }
      return clamp(finite(read({ signal: r.signal, band: f.channel, reference: r.reference })) / 2, 0, 1);
    }
    if (type === 'envelope' && f.module === loudId) return f.port === 'out' ? env.out : env.level;
    if (type === 'clock') return f.port === 'beat' || f.port === 'bar' || f.port === 'phrase' ? ticks[f.port] : (outputs['clock.rate'] ?? st.clock.rate);
    // A Grid's channel: each point's index, distance, angle, row or column (0..1).
    if (type === 'grid') return finite(st.grids[f.module]?.grid.channels[f.channel ?? 0]?.[f.port as 'index']);
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
    // A new track's: one strike as it starts (the visuals opening on one too), its knobs the first channel's.
    const onTrack = gate?.from.port === 'newtrack';
    const kind = gate && STRIKES[gate.from.port];
    if (!gate || (!kind && !onTrack)) return 0;
    if (onTrack) ch = 0;
    else if (ch === undefined) return 0;
    const k = moduleOf.get(id)?.knobs ?? {};
    if (onTrack) {
      TRACK_STRIKE.since = st.track.key === undefined ? Infinity : Math.max(0, (opts.now - st.track.at) / 1000);
      ADSR_KNOBS.attack = 0;
      ADSR_KNOBS.decay = finite(k[DECAY_AT[0]]);
      ADSR_KNOBS.sustain = finite(k[SUSTAIN_AT[0]]);
      ADSR_KNOBS.release = finite(k[RELEASE_AT[0]], 0.25);
      ADSR_KNOBS.retrigger = 0;
      return adsrOf(TRACK_STRIKE, ADSR_KNOBS) * finite(k.amount, 1);
    }
    const strike = m.strikes?.[ch!]?.[kind!] ?? NO_STRIKE;
    const reference = adsrReading(id);
    const velocity = (v: number) => (reference === 'absolute' ? v : finite(read({ signal: readingOf(gate.from.port).signal, band: ch!, reference, value: v })));
    STRUCK.since = strike.since;
    STRUCK.velocity = velocity(strike.velocity);
    if (strike.next) {
      STRUCK_NEXT.until = strike.next.until;
      STRUCK_NEXT.velocity = velocity(strike.next.velocity);
    }
    STRUCK.next = strike.next ? STRUCK_NEXT : null;
    ADSR_KNOBS.attack = finite(k[ATTACK_AT[ch]]);
    ADSR_KNOBS.decay = finite(k[DECAY_AT[ch]]);
    ADSR_KNOBS.sustain = finite(k[SUSTAIN_AT[ch]]);
    ADSR_KNOBS.release = finite(k[RELEASE_AT[ch]], 0.15);
    ADSR_KNOBS.retrigger = finite(k.retrigger, 0);
    return adsrOf(STRUCK, ADSR_KNOBS) * finite(k.amount, 1);
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
  const shapeFor = (c: CableV2): Shape => {
    const kept = plan.shapes.get(c);
    if (kept && kept.v === c.value) return kept.shape;
    const shape = shapeOf(c.value);
    plan.shapes.set(c, { v: c.value, shape });
    return shape;
  };
  const walkOf = (c: CableV2) => (shapeFor(c).walks ? finite(opts.walk?.[c.id], 1) : 1);
  /** A cable into a knob as the knob takes it: a polyphonic one (from a band's signal, an ADSR's or a VCA's channels,
   *  with no channel of its own: VCV's polyphonic cable) as one a channel, each its band's; any other as it is. */
  const expanded = plan.expanded;
  const perChannel = (cables: readonly CableV2[]): CableV2[] => expanded.get(cables) ?? expanded.set(cables, expand(cables)).get(cables)!;
  const expand = (cables: readonly CableV2[]): CableV2[] =>
    cables.flatMap((c) => {
      if (c.from.channel !== undefined) return [c];
      const type = typeOf.get(c.from.module) ?? c.from.module;
      const poly = (type === 'music' && BAND_PORTS.has(readingOf(c.from.port).signal)) || type === 'adsr' || CHANNELLED.has(type);
      return poly ? Array.from({ length: CHANNELS }, (_, ch) => ({ ...c, from: { ...c.from, channel: ch } })) : [c];
    });
  // The Screen's flash guard (WCAG 2.3.1; claude-632's limiter on the output, step 3): fixed, keyed by the flashes the
  // last frame made, a tenth at most held back; no knob in it.
  const guard =
    plan.first.has('screen') && opts.meters
      ? Math.max(0.1, stepDucker((st.duckers['screen|guard'] ??= newDucker()), 1, Math.max(opts.meters.flash, opts.meters.redFlash), FLASH_GUARD, step).gain)
      : 1;
  // The VCAs', Merges' and Duckers' channels: each one's buffer, last frame's until it is worked out this frame.
  const channelsOf = (id: string) => st.poly[id] ?? NO_CHANNELS;
  /** What a cable gives a port on channel `ch` (a knob's cable's rule: its curve at its source's x, times its depth
   *  and its walk); a VCA's or a Merge's source taken as it is, along its straight line where the value is a number. */
  // (A channel-less cable from a band's signal comes as its channel's, from the plan.)
  const giveOf = (entry: Entry, ch: number): number => {
    const { c, type } = entry;
    if (entry.v !== c.value) [entry.v, entry.shape] = [c.value, shapeOf(c.value)];
    const sh = entry.shape;
    if (PROCESSORS.has(type)) {
      const v = channelsOf(c.from.module)[CHANNELLED.has(type) ? (c.from.channel ?? ch) : 0];
      return (sh.table ? heightOf(sh, clamp(v, 0, 1)) : sh.end * v) * depthOf(c) * (sh.walks ? walkOf(c) : 1);
    }
    const x = xOf(c, type);
    return x === null ? 0 : heightOf(sh, x) * depthOf(c) * (sh.walks ? walkOf(c) : 1);
  };
  /** What a port takes on channel `ch`: its cables' sum (a cable with no channel of its own reaches every channel). */
  const sumOf = (reaching: Entry[][] | undefined, ch: number): number => {
    let sum = 0;
    if (reaching) for (const c of reaching[ch]) sum += giveOf(c, ch);
    return sum;
  };
  const takes = (module: string, port: string, ch: number): number => sumOf(plan.ports.get(module)?.get(port), ch);
  const plugged = (module: string, port: string, ch: number) => (plan.ports.get(module)?.get(port)?.[ch].length ?? 0) > 0;
  /** A VCA (in × gain, the gain its knob plus what its gain input takes, 0 to the knob's most; its input normalled to
   *  1) or a Merge (its channels' sum, mean, largest or geometric mean), channel by channel. */
  function process(x: Module) {
    const out = SCRATCH;
    if (x.type === 'ducker') {
      // A Ducker (compressor.ts): its input (normalled to 1) held down while its key is over its threshold (the
      // threshold's gain input moving it, as the music's activity does a budget).
      const k = x.knobs;
      const knobs = {
        threshold: baseOf(x, 'threshold', 1) * gainOf(x.id, 'threshold'),
        depth: finite(k.depth, 0),
        attack: finite(k.attack, 0),
        hold: finite(k.hold, 0),
        release: finite(k.release, 1),
        hysteresis: finite(k.hysteresis, 1),
      };
      const input = plugged(x.id, 'in', 0) ? takes(x.id, 'in', 0) : 1;
      out.fill(stepDucker((st.duckers[x.id] ??= newDucker()), input, takes(x.id, 'key', 0), knobs, step).out);
      (st.poly[x.id] ??= new Float64Array(CHANNELS)).set(out);
      return;
    }
    if (x.type === 'shaper') {
      // A Shaper (0057 §7f): each channel the curve's height at what its input takes, as a cable's curve (curves.ts);
      // its curve made again only when its knobs change.
      const k = x.knobs;
      const key = `${k.h}|${k.h1x}|${k.h1y}|${k.h2x}|${k.h2y}`;
      let kept = plan.shapers.get(x.id);
      if (kept?.key !== key) {
        const h = finite(k.h, 1);
        const curve = { h1: [finite(k.h1x, 1 / 3), finite(k.h1y, h / 3)] as [number, number], h2: [finite(k.h2x, 2 / 3), finite(k.h2y, (2 * h) / 3)] as [number, number], h };
        plan.shapers.set(x.id, (kept = { key, shape: shapeOf(curve) }));
      }
      const into = plan.ports.get(x.id)?.get('in');
      for (let ch = 0; ch < CHANNELS; ch++) out[ch] = heightOf(kept.shape, sumOf(into, ch));
      (st.poly[x.id] ??= new Float64Array(CHANNELS)).set(out);
      return;
    }
    if (x.type === 'vca') {
      const gain = finite(x.knobs.gain, 1);
      const channels = plan.polyphonic.has(x.id) ? CHANNELS : 1;
      const ports = plan.ports.get(x.id);
      const into = ports?.get('in');
      const cv = ports?.get('cv');
      for (let ch = 0; ch < channels; ch++) out[ch] = (into && into[ch].length ? sumOf(into, ch) : 1) * clamp(gain + sumOf(cv, ch), 0, 2);
      if (channels === 1) out.fill(out[0], 1);
    } else {
      const into = plan.ports.get(x.id)?.get('in');
      const mode = MERGE_MODES[clamp(Math.round(finite(x.knobs.mode, 1)), 0, MERGE_MODES.length - 1)];
      let one = mode === 'max' ? -Infinity : mode === 'geometric' ? 1 : 0;
      for (let ch = 0; ch < CHANNELS; ch++) {
        const v = sumOf(into, ch);
        if (mode === 'max') one = Math.max(one, v);
        else if (mode === 'geometric') one *= v;
        else one += v;
      }
      if (mode === 'mean') one /= CHANNELS;
      else if (mode === 'geometric') one = one ** (1 / CHANNELS);
      out.fill(one);
    }
    (st.poly[x.id] ??= new Float64Array(CHANNELS)).set(out);
  }
  /** A level knob's gain input (normalled to 1), 0..1. */
  const gainOf = (module: string, knob: string) => {
    const reaching = plan.gains.get(module)?.get(knob);
    return reaching ? clamp(sumOf(reaching, 0), 0, 1) : 1;
  };
  /** A step knob's trigger input: whether it rose past a half this frame (null: nothing plugged into it). */
  const fired = (module: string, knob: string): boolean | null => {
    const reaching = plan.nexts.get(module)?.get(knob);
    if (!reaching) return null;
    const key = `${module}|${knob}${NEXT}`;
    const v = sumOf(reaching, 0);
    const was = finite(st.triggers[key], 0);
    st.triggers[key] = v;
    return v > 0.5 && was <= 0.5;
  };


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
        const r = readingOf(c.from.port);
        const v =
          type === 'adsr' ? adsrOut(c.from.module, c.from.channel) : r.reference === 'absolute' ? (xOf(c) ?? 0) : finite(read({ signal: r.signal, band: c.from.channel, reference: r.reference }));
        sum += (sh.table ? 2 * heightOf(sh, clamp(v / 2, 0, 1)) : sh.end * v) * w;
        continue;
      }
      const x = xOf(c);
      if (x !== null) sum += heightOf(sh, x) * w;
    }
    return sum;
  };
  // The macros' targets: a target's base lerp(from, to, the macro's value) before its cables act (not a step's).
  for (const id of Object.keys(st.macros)) if (!macroIds.has(id)) delete st.macros[id];
  const macros: Record<string, number> = {};
  const macroOf = (x: Macro) => (x.id in macros ? macros[x.id] : finite(st.macros[x.id], clamp(finite(x.value), 0, 1)));
  const nexts: NextRequest[] = [];
  const modulesNow = plan.modulesNow;

  // The macros and the modulators, in the order their cables (and a modulator's macros) need.
  const knobs: Record<string, Record<string, number>> = {};
  for (const next of plan.order) {
    if (next.macro) {
      macros[next.id] = moved(next.id, MACRO_KNOB, clamp(finite(next.macro.value), 0, 1), into.get(`${next.id}|value`) ?? []);
      continue;
    }
    const x = next.module!;
    if (PROCESSORS.has(x.type)) {
      process(x);
      continue;
    }
    const t = modulatorType(x.type)!;
    const k = knobsOf(x, t);
    for (const port of t.inputs ?? []) k[port] = portSum(x.id, port);
    // Its trigger inputs, only where plugged in (an unplugged clock is none, not 0); a reset normalled to the Clock's bar.
    for (const port of t.triggers ?? []) {
      if (into.has(`${x.id}|${port}`)) k[port] = portSum(x.id, port);
      else if (port === 'reset' && clockModule) k[port] = ticks.bar;
      else delete k[port];
    }
    knobs[x.id] = k;
    outputs[x.id] = st.modulators[x.id].run.step(step, read, k);
  }
  for (const id of Object.keys(st.modulators)) if (id in outputs) st.values[id] = outputs[id];
  for (let i = 0; i < plan.processorIds.length; i++) outputs[plan.outKeys[i]] = channelsOf(plan.processorIds[i])[0];
  outputs['screen.guard'] = guard;
  for (const id of Object.keys(macros)) st.macros[id] = macros[id];

  // The chain's columns: what their cables give, by the routes' rules.
  const got = st.got;
  got.whole.fill(0);
  got.level.fill(1);
  for (const own of got.band) own.fill(0);
  const n = got.band.length;
  if (!plan.columnCables) {
    plan.columnCables = [];
    for (const [key, cables] of into) {
      const a = colAt(key);
      if (a === undefined) continue;
      plan.columnCables.push({ a, cables: perChannel(cables).map((c) => ({ c, type: typeOf.get(c.from.module) ?? c.from.module, v: c.value, shape: shapeOf(c.value) })) });
      if (!IS_LEVEL[a] && !IS_PART[a] && perChannel(cables).some((c) => c.from.channel !== undefined) && !plan.bandColumns.includes(a)) plan.bandColumns.push(a);
    }
  }
  for (const { a, cables } of plan.columnCables) {
    for (const entry of cables) {
      const c = entry.c;
      const x = xOf(c, entry.type);
      if (x === null) continue;
      const band = c.from.channel ?? null;
      if (band !== null && !(band >= 0 && band < n)) continue;
      if (entry.v !== c.value) [entry.v, entry.shape] = [c.value, shapeOf(c.value)];
      const sh = entry.shape;
      const y = heightOf(sh, x);
      const w = walkOf(c);
      const e = depthOf(c);
      // Cables only add (0057 §7e): into a level column, to its share of its setting (1: as set), a band's as the
      // bands' mean; what multiplies it comes in at its gain input.
      if (IS_LEVEL[a]) got.level[a] += (e * y * w) / (band === null ? 1 : n);
      else if (band === null) got.whole[a] += e * y * w;
      else got.band[band][a] += e * y * w;
    }
  }
  // The bands' mean (a moment's: their largest) into the whole, for the columns a band's cable reaches (the rest have none).
  for (const a of plan.bandColumns) {
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
  if (!plan.columnInputs) {
    plan.columnInputs = [];
    for (const key of into.keys()) {
      const gain = key.endsWith(GAIN);
      if (!gain && !key.endsWith(NEXT)) continue;
      const base = key.slice(0, -(gain ? GAIN : NEXT).length);
      const a = colAt(base);
      if (a === undefined) continue;
      const [module, knob] = base.split('|');
      plan.columnInputs.push({ a, module, knob, gain });
    }
  }
  for (const { a, module, knob, gain } of plan.columnInputs) {
    if (gain && IS_LEVEL[a]) got.level[a] *= gainOf(module, knob);
    // A swing column's gain scales its modulation (Bitwig's modulation scaling): its cables, a part's own among them.
    if (gain && !IS_LEVEL[a] && !IS_MOMENT[a]) {
      const g = gainOf(module, knob);
      got.whole[a] *= g;
      for (const own of got.band) own[a] *= g;
    }
    if (!gain && IS_MOMENT[a] && fired(module, knob)) {
      firedNow[ASPECTS[a]] = true;
      nexts.push({ module, knob });
    }
  }

  // Each swing column's knob, its resting value (0057 §7e, VCV's rule: the knob the offset, the cables adding to it).
  if (!plan.columnOffsets) {
    plan.columnOffsets = [];
    for (const offset of COLUMN_OFFSETS) {
      const [type, knob] = offset.split('.');
      const rel = type === 'flame' || type === 'trails' ? type : `show/${type}`;
      const a = colAt(`${at}${rel}|${knob}`);
      if (a !== undefined) plan.columnOffsets.push({ a, rel, knob });
    }
  }
  for (const { a, rel, knob } of plan.columnOffsets) got.whole[a] += knobOf(inChain(rel), knob, 0);

  // The chain's own modules, each as its gain inputs hold it (the sidechains' Duckers, 0057 §7e): the Clock's speed
  // (its pace), the Accent's accent (the flash guard: its swing, and its courses slower), the Flame's evolving times
  // (a time's gain stretches it). Levers given (today's sidechaining, until the patch has its Duckers) act with them.
  const held: Levers = {
    ...L,
    pace: L.pace * gainOf(ofType('clock')?.id ?? 'clock', 'speed'),
    swing: L.swing * gainOf(ofType('accent')?.id ?? 'accent', 'accent'),
    evolution: L.evolution / Math.max(0.01, gainOf(flameModule?.id ?? 'flame', 'evolution')),
  };
  const clock = stepClock(st.clock, m.bps, m.playing, chainSettings.speed, held.pace, opts.still, step);
  // The picture's level: the Tone map's Level knob as its gain input holds it (the pause's dimming, wired), where the
  // patch has the Tone map; else the Envelope's glow (today's).
  const tonemap = p.modules.find((x) => x.type === 'tonemap');
  const glow = tonemap ? finite(tonemap.knobs.level, 1) * gainOf(tonemap.id, 'level') : env.glow;
  outputs['clock.rate'] = clock;
  const trackInto = (a: Aspect) => {
    let sum = 0;
    for (const c of (plan.trackCables[a] ??= columnOf(a).flatMap((key) => into.get(at + key) ?? []))) if (trackAdsr(c.from.module, typeOf, into)) sum += depthOf(c) * heightOf(shapeOf(c.value), finite(xOf(c))) * walkOf(c);
    return sum;
  };
  const aspects = !flameModule
    ? null
    : stepChain(
        st.chain,
        m,
        chainSettings,
        { given: got, track: raw, bands, lit: env.level, envelope: env.out, glow, clock, trackWalk: { look: trackInto('look'), behaviour: trackInto('behaviour') }, fired: firedNow },
        step,
        { ...opts, levers: held },
      );

  // Every other module's knobs, as the evaluator moves a knob.
  if (!plan.others) {
    plan.others = [];
    for (const x of p.modules) {
      const t = types[x.type];
      if (!t || live.has(x.id) || PROCESSORS.has(x.type)) continue;
      if (t.knobs.some((knob) => colAt(`${x.id}|${knob.id}`) !== undefined)) continue;
      plan.others.push({ x, t });
    }
  }
  for (const { x, t } of plan.others) if (!knobs[x.id]) knobs[x.id] = knobsOf(x, t);
  // Each Dice: a roll as its trigger rises past a half (its `done` a trigger the same frame), seeded by its seed input
  // (0..1 back to the 32 bits it came from; none plugged: the frame's time), over its scope.
  const dice: DiceRoll[] = [];
  for (const x of plan.dice) {
    const v = sumOf(plan.ports.get(x.id)?.get('trigger'), 0);
    const key = `${x.id}|trigger`;
    const rose = v > 0.5 && finite(st.triggers[key], 0) <= 0.5;
    st.triggers[key] = v;
    outputs[`${x.id}.done`] = rose ? 1 : 0;
    if (!rose) continue;
    const seeded = plan.ports.get(x.id)?.get('seed');
    const seed = seeded && seeded[0].length ? Math.floor(clamp(sumOf(seeded, 0), 0, 1 - 2 ** -33) * 4294967296) : Math.floor(opts.now) >>> 0;
    dice.push({ module: x.id, seed, scope: RANDOMIZE_SCOPES[clamp(Math.round(finite(x.knobs.scope, 0)), 0, RANDOMIZE_SCOPES.length - 1)] });
  }
  // What reaches each Fluid's `at`: the points output cabled into it (a Flame's, a Grid's), else the Flame's (its normal).
  const pointsOf = (r: PortRef): readonly (Point | null)[] | null => {
    const type = typeOf.get(r.module);
    if (type === 'flame' && r.port === 'points') return opts.flamePoints ?? null;
    if (type === 'grid' && r.port === 'points') return (st.grids[r.module]?.grid.points ?? []).map((q): Point => [q.x * 0.5 + 0.5, q.y * 0.5 + 0.5]);
    return null;
  };
  const pointsAt: Record<string, readonly (Point | null)[]> = {};
  for (const x of plan.fluids) {
    const cabled = plan.intoOf.get(x.id)?.get('at')?.[0];
    const got = cabled ? pointsOf(cabled.from) : flameModule ? (opts.flamePoints ?? null) : null;
    if (got) pointsAt[`${x.id}.at`] = got;
  }
  return { aspects, knobs, outputs, macros, nexts, points, pointsAt, dice };

  /** A module's knobs as its macros and cables move them. */
  /** A knob's base as its macros set it, along its taper (an exponential knob's geometrically: 0057's Wild). */
  function macroed(module: string, knob: Knob | undefined, base: number): number {
    for (const g of (knob && plan.targetsOf.get(module)?.get(knob.id)) || []) {
      const at = macroOf(g.macro);
      base = knob?.taper === 'exp' && g.from > 0 && g.to > 0 ? g.from * (g.to / g.from) ** at : g.from + (g.to - g.from) * at;
    }
    return base;
  }
  /** An engine module's knob's base: as set, its macros' (the Ducker's threshold under Wild). */
  function baseOf(x: Module, id: string, or: number) {
    return macroed(x.id, (types[x.type] ?? ENGINE_TYPES[x.type])?.knobs.find((k) => k.id === id) ?? ({ id } as Knob), finite(x.knobs[id], or));
  }

  function knobsOf(x: Module, t: ModuleType): Record<string, number> {
    const k: Record<string, number> = { ...x.knobs };
    for (const knob of t.knobs) {
      let base = finite(x.knobs[knob.id], knob.default);
      if (knob.kind !== 'step') base = macroed(x.id, knob, base);
      k[knob.id] = moved(x.id, knob, base, plan.intoOf.get(x.id)?.get(knob.id) ?? NO_CABLES);
    }
    return k;
  }

  /** A knob moved from its base by its cables (evaluate.ts's rule, cables only adding: 0057 §7e; a band's cable
   *  into a knob not a band's own, the bands' mean and geometric mean). */
  function moved(module: string, k: Knob, base: number, cables: CableV2[]): number {
    // A step moves on only as a trigger fires its Next (0057 §7e), to its next step; its cables move nothing.
    if (k.kind === 'step') {
      if (fired(module, k.id)) nexts.push({ module, knob: k.id });
      return clamp(base, k.min, k.max);
    }
    base = k.time ? base / Math.max(0.01, gainOf(module, k.id)) : base * gainOf(module, k.id);
    if (!cables.length) return clamp(base, k.min, k.max);
    const ownBand = /\.\d+$/.test(k.id);
    let swing = 0;
    let bandSwing = 0;
    for (const c of perChannel(cables)) {
      const x = xOf(c);
      if (x === null) continue;
      const sh = shapeOf(c.value);
      const y = heightOf(sh, x);
      const w = walkOf(c);
      const e = depthOf(c);
      if (c.from.channel !== undefined && !ownBand) bandSwing += e * y * w;
      else swing += e * y * w;
    }
    return clamp(base + (swing + bandSwing / BANDS) * ((k.max - k.min) / 2), k.min, k.max);
  }
}
