// The one engine (decision 0057, step 2): every value and trigger of a v2 patch, a frame at a time, over its typed
// cables. Music, Track and the Envelope first (what every cable reads and what scales it); then the modulators, VCAs
// and Merges in the order their cables need (a cycle reads last frame's value, VCV Rack's rule); then what the cables
// give each of the Cratarr device's columns, by the routes' rules (routes.ts give: a swing sums, a level lets through, a
// band's cable into a part's column is that part's own, into another the bands' mean, a moment's their largest; a
// column's scale is in its cables' amounts, 0057 §7e), a level column's gain input and a moment column's trigger; then
// the chain's own modules (chain.ts); and every other module's knobs as their cables move them (evaluate.ts's rule).
// What aspects.ts and evaluate.ts did between them, in one place and all of it wired.

import { throughMacros } from '../macro-map';
import { effectsNow, settingsReport, type Report } from '../effects';
import type { StageValues } from '../stage-values';
import { asSet } from '../v2';
import { DIVE_MOST, TUNNEL_MOST } from '../zoom';
import { effectSlotsOf } from './settings-view';
import { heightOf, shapeOf, type Shape } from '../evaluate';
import { seedOf } from '../flame';
import type { Levers } from './sidechain';
import { modulatorOf, type ModulatorType } from '../modulators/modulators';
import type { Music } from '../music-core';
import { ASPECT_KNOB, type Knob, type Macro, type Module, type ModuleType, type Modulator, type SignalRef } from '../patch';
import { COLUMN_OFFSETS, ENGINE_MODULES, MOST_POINTS, RANDOMIZE_SCOPES, READINGS, SHOW_PARTS, type CableV2, type PatchV2, type PortRef } from '../ports';
import { A, AMOUNT_ASPECTS, ASPECTS, B, BAND_SIGNALS, KNOB_ASPECTS, LEVEL_ASPECTS, MOMENT_ASPECTS, PART_ASPECTS, T, TRACK_SIGNALS, given, given as columnsFor, type Aspect, type Given } from '../routes';
import { BANDS, type VisualSettings } from '../settings.svelte';
import { newFlame, stepFlame, type FlameNow, type FlameState } from './chain';
import { newFlameNode, stepFlameNode, type FlameNodeState, type FlameView } from './flame-node';
import type { PaletteColour } from '../palette';
import type { Flow, Force } from '../fluid';
import { newStirring, stir, type Stirring } from '../stirring';
import { fluidGains } from './fluid-gains';
import { newDucker, stepDucker, type DuckerState } from './compressor';
import { newClockTriggers, stepClockTriggers, type ClockTriggerState, type ClockTriggers } from './clock';
import { gridOf, type Grid, type GridPoint } from './grid';
import { FLASH_GUARD } from './sidechain';
import type { MeterValues } from '../meters';
import { ownKnob, slotModules, slotOf, SLOT_IDS, type Slot } from './effect-slots';
import { musicView } from './music-signals';
import { flowAt, PLAIN, WAVES, type FieldShape } from '../field';
import { effectKnobIds, effectOfType } from '../effect-modules';
import { engineOf } from '../engine-module';
import { hazeOf, NO_STRIKE, adsrOf, courseStep, newAccent, stepAccent, type AccentState, newClock, newEnvelope, newTrack, stepClock, stepEnvelope, stepTrack, type ClockState, type EnvelopeState, type TrackState } from './modules';

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
/** The column a chain module's knob is by its module's type (`type|knob`), whichever module and wherever it sits: the
 *  one engine's (step 5), each module its own. The effects' slots are not types (effect-slots.ts). */
const TYPE_COLUMN = new Map<string, number>();
for (const [a, [m, k]] of Object.entries(ASPECT_KNOB) as [Aspect, [string, string]][]) {
  if (/^fx\d$/.test(m) || m === 'loop-effect') continue;
  if (m !== 'show') TYPE_COLUMN.set(`${m}|${k}`, A[a]);
  else if (k === 'size') TYPE_COLUMN.set('flame|zoom', A[a]);
  else {
    const part = SHOW_PARTS.find((x) => x.knob === k);
    if (part) TYPE_COLUMN.set(`${part.type}|${k}`, A[a]);
  }
}
/** Each effect slot's moment (its effect stepping on). */
const SLOT_MOMENT: Record<Slot, Aspect> = { fx1: 'fx1', fx2: 'fx2', fx3: 'fx3', 'loop-effect': 'fxLoop' };
/** Each chain type's knobs that are columns. */
const TYPE_KNOBS = new Map<string, string[]>();
for (const key of TYPE_COLUMN.keys()) {
  const [type, knob] = key.split('|');
  (TYPE_KNOBS.get(type) ?? TYPE_KNOBS.set(type, []).get(type)!).push(knob);
}
/** A chain module's own columns: what its cables give each of its aspects (as `Given`), kept per module; which knob is
 *  which column; an effect's (its Amount a level, its two controls swings about where they are set, within 0..1). */
type Own = { id: string; type: string; got: Given; columns: Map<string, number>; effect: boolean };

/** An effect module's knobs as columns (an effect's own or an engine's): its Amount a level, its two controls swings
 *  about where they are set (effects.ts effectsNow's rule); none for any other module. */
function effectColumns(type: string): Map<string, number> | null {
  const e = effectOfType(type);
  const names = e ? effectKnobIds(e) : engineOf(type) ? ['k1', 'k2'] : null;
  if (!names) return null;
  return new Map([['amount', A.fx1Amount], [names[0], A.fx1K1], [names[1], A.fx1K2]]);
}
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
  /** The track's signals as the cables read them raw (the loudness the Envelope's level, the Track pulse in them). */
  signals: Float32Array;
  standIn: Float32Array[];
  /** Each chain module's Accent courses (`<module>|<knob>`): its swings followed slower, the accent on what is left. */
  accents: Record<string, AccentState>;
  /** Each Flame's own state (its node's, step 5), by module. */
  flames: Record<string, FlameState>;
  /** Each Flame's look, framing and place on the screen (flame-node.ts), where the engine runs with a view. */
  flameNodes: Record<string, FlameNodeState>;
  /** The Fluid's stirring (stirring.ts): its kicks under way, the hits and beats it last kicked for. */
  stirring: Stirring;
  /** Each Field's time run on. */
  fieldPhases: Record<string, number>;
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
  grids: Record<string, { key: string; grid: Grid; offsets?: [number, number][] }>;
};

export function newEngine(): EngineState {
  return {
    envelope: newEnvelope(),
    clock: newClock(),
    track: newTrack(),
    signals: new Float32Array(TRACK_SIGNALS.length),
    standIn: Array.from({ length: BANDS }, () => {
      const v = new Float32Array(BAND_SIGNALS.length);
      v[B.level] = 0.5;
      return v;
    }),
    accents: {},
    flames: {},
    flameNodes: {},
    stirring: newStirring(),
    fieldPhases: {},
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

/** A module's face outputs' keys: a Gate's input, a Ducker's reduction, a Merge's channels in. */
type FaceKeys = { input: string; reduction: string; in: string[] };
const faceKeys = (id: string): FaceKeys => ({ input: `${id}.in`, reduction: `${id}.reduction`, in: Array.from({ length: CHANNELS }, (_, ch) => `${id}.in.${ch}`) });

/** A step knob to move on (its module flattened). */
/** A step whose Next fired this frame (0057 §7e), and the step it moves on to (past its last, its first). */
export type NextRequest = { module: string; knob: string; to: number };

/** What the Cratarr device gives the light's stages this frame (0.10, the architecture audit's visuals step 3: no
 *  summary between the modules and the renderer): each stage's values as the device's modules make them, those a
 *  stage's own module does not give its device's defaults (Trails' keep, the Curve's haze, Tone map's flash, the
 *  Fluid's share, Tunnel and Dive at their settings); the effects as their slots, levels and routes set them; the
 *  Flame's turn, size and lights. The renderer lays each stage's own modules' values over them (stage-values.ts). And
 *  what the meters count of it: the motions the waves, the zooms and the effects make, a new symmetry. */
export type DeviceFrame = {
  stages: StageValues;
  meter: { waves: number; fluid: number; tunnel: number; dive: number; effects: Report; symmetry: boolean };
};

export type EngineFrame = {
  /** What the light's stages draw with this frame; null where the patch has no Cratarr device (a blank slate: nothing
   *  of the light is drawn). */
  device: DeviceFrame | null;
  /** Every module's knobs as their cables move them (the chain's: its settings, its columns given in `aspects`). */
  knobs: Record<string, Record<string, number>>;
  /** What each chain module's node draws with this frame (step 5, the one engine), from its own knobs: a Show part's
   *  (Tone map's flash, Saturate's saturation, Curve's gamma and floor, Blur's smoothing, Bend's waves). */
  params: Record<string, NodeParams>;
  /** Every value output this frame, by `module.port`: the same object each frame while the patch is the same (copy what
   *  is to be kept). */
  outputs: Record<string, number>;
  /** Each macro's value this frame. */
  macros: Record<string, number>;
  /** The steps past the chain whose cables ask for a moment (morph.ts takes it at the module's neutral). */
  /** The steps whose Next a trigger fired this frame (0057 §7e): each to move on to its next step. */
  nexts: NextRequest[];
  /** Each Field's flow now (field.ts): its scale and how far its time has run (Speed integrated, so a turned knob
   *  never jumps it). */
  fields: Record<string, { scale: number; phase: number }>;
  /** Each Grid's points (its `points` output, for Instance: the compositor). */
  points: Record<string, GridPoint[]>;
  /** What reaches each points input (`module.port`: a Fluid's `at`, where it pushes): what is cabled into it (a Flame's
   *  points, a Grid's), else what it is normalled to (the Flame's points). For the compositor, which runs the sim. */
  pointsAt: Record<string, readonly (Point | null)[]>;
  /** The Dice whose trigger rose this frame (0057 §7f): each one's seed and scope, for the page to randomize the patch
   *  with (randomize.ts: over frames, as one undoable change through the store). */
  dice: DiceRoll[];
  /** What each Flame draws with this frame (flame-node.ts: its look, framed), where the engine runs with a view. */
  flames: Record<string, FlameView>;
  /** The Cratarr device's Fluid stirred this frame (its pushes and its flow, for the light loop), with a view. */
  fluid: { forces: Force[]; flow: Flow } | null;
};

/** A chain module's node values (EngineFrame.params), by its type. */
export type NodeParams = {
  flash?: number;
  saturation?: number;
  gamma?: number;
  floor?: number;
  smoothing?: number;
  waves?: number;
  /** Tone map's exposure (its Auto exposure). */
  exposure?: number;
  /** Trails: the share of the light kept a frame. */
  keep?: number;
  /** Fluid: its share (of the fluid at its 100 %), the warp it gives, its facets. */
  fluid?: number;
  warp?: number;
  facets?: number;
  /** Tunnel's and Dive's levels. */
  tunnel?: number;
  dive?: number;
  /** The Flame's size (its Zoom, its Accent course its own). */
  scale?: number;
  /** The Flame's parts, turn and lights (its own state, stepFlame). */
  flame?: FlameNow;
};

/** A Dice's roll: which, the seed its seed input gives (Music's `seed`: the track's own), its scope. */
/** A Dice's roll: its seed, its scope, and how far from where each knob is (`reach` 0..1: 1, anywhere in its range). */
export type DiceRoll = { module: string; seed: number; scope: (typeof RANDOMIZE_SCOPES)[number]; reach: number };

export type EngineOptions = {
  now: number;
  fluidFull: number;
  /** Reduced motion, the device's (decision 0057: kept on purpose; the architecture audit, 2026-10-29, found it
   *  dropped): as aspects.ts had it, a quarter of the turning, the beats', downbeats' and sections' share halved, the
   *  Flame's parts steady with a slower morph and half the lights, and no moments. */
  still?: boolean;
  levers?: Levers;
  /** Each walking cable's factor now, by its id (1: as set). */
  walk?: Record<string, number>;
  /** What the Meter measured of the last frame drawn (meters.ts; the precheck's measure()): the sidechains' keys. */
  meters?: MeterValues;
  /** The Flame's parts where they are on the screen (0..1, y up; null: a part not drawn), as the page measures them: it
   *  holds the drawn flame and its framing. The Flame's `points` output (0057, the modular Fluid). */
  flamePoints?: readonly (Point | null)[];
  /** Where the Flame is and how much it covers (its x, y, size, coverage outputs: the Transform's units, a square
   *  space whose short side is −1..1, its centre 0), as the loop last measured it. */
  flameFrame?: { x: number; y: number; size: number; coverage: number } | null;
  /** The picture's width over its height (where a Flame's parts are, in a Grid's frame). */
  aspect?: number;
  /** Run with a view (the page's frame loop): each Flame's look, framing and place on the screen worked out by its node
   *  (flame-node.ts), from the playing track's cover colours as the page has them and how long this frame's change of
   *  look glides (a new track's, the dice's, a step's; null: as a knob's). Its parts and outputs are then its own, not
   *  `flamePoints` and `flameFrame` (a lab's or a test's, without a view). */
  view?: { cover: { key: string | null; colours: PaletteColour[] | null }; glide: number | null };
  /** The Fluid's flow where a Grid drifting on it is (the frame's units a second), as the renderer last read it back. */
  fluidFlow?: (x: number, y: number) => [number, number];
  /** The device's most brightness (its Brightness setting): Show's ceiling. */
  ceiling?: number;
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
  /** Whether something reads a Flame's parts on the screen: a Fluid (its pushes, normalled to them), a cable from its
   *  points (a Grid's or Streamlines' seeds). */
  readsFlameParts: boolean;
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
  /** Each module's ports' cables reaching each channel (a channel-less one from a band's signal as that channel's). */
  ports: Map<string, Map<string, Entry[][]>>;
  /** Each module's knobs' gain inputs and next triggers, by the knob. */
  gains: Map<string, Map<string, Entry[][]>>;
  nexts: Map<string, Map<string, Entry[][]>>;
  /** Each cable's shape, as its value was when last asked. */
  shapes: WeakMap<CableV2, { v: CableV2['value']; shape: Shape }>;
  /** The modulators' ids; each Music row's reading, and the knob value it was read at. */
  live: Set<string>;
  readings: Map<string, { at: unknown; knob: string; reading: Reading }>;
  processorIds: string[];
  outKeys: string[];
  /** Each chain module's own columns (step 5): its cables per column, its band columns, its gains, its offsets. */
  owns: { own: Own; cables: { a: number; cables: CableV2[] }[]; bands: number[]; gains: { a: number; knob: string }[]; offsets: { a: number; knob: string }[] }[] | null;
  trackCables: Partial<Record<Aspect, CableV2[]>>;
  /** Each module's ports' cables, by module then port; the modules past the chain whose knobs the engine moves
   *  (worked out at the first frame). */
  intoOf: Map<string, Map<string, CableV2[]>>;
  others: { x: Module; t: ModuleType }[] | null;
  /** Each module's knobs' macro targets, by module then knob. */
  targetsOf: Map<string, Map<string, { macro: Macro; from: number; to: number }[]>>;
  /** The modules' face outputs' keys (their faces read these), made once a plan. */
  faceKeys: Record<string, FaceKeys>;
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
/** Each Field's Waves cables, a wave's (its channel's) each, by the cables into it. */
const WAVE_CABLES = new WeakMap<readonly CableV2[], CableV2[][]>();
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
  // A macro's targets after it: a module's knobs, and a device's macro it maps (0077: one level down).
  for (const macro of p.macros) for (const t of macro.targets) if (ids.has(t.module) && t.module !== macro.id && (!macroIds.has(t.module) || t.knob === MACRO_KNOB.id)) needs.get(t.module)!.add(macro.id);
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
    readsFlameParts: p.modules.some((x) => x.type === 'fluid') || p.cables.some((c) => typeOf.get(c.from.module) === 'flame' && c.from.port === 'points'),
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
    polyphonic: polyphonicOf(p, typeOf, into),
    ports: byPort(into, typeOf, ''),
    gains: byPort(into, typeOf, GAIN),
    nexts: byPort(into, typeOf, NEXT),
    shapes: new WeakMap(),
    live: new Set(running.map((x) => x.id)),
    readings: new Map(),
    processorIds: p.modules.filter((x) => PROCESSORS.has(x.type)).map((x) => x.id),
    outKeys: p.modules.filter((x) => PROCESSORS.has(x.type)).map((x) => `${x.id}.out`),
    owns: null,
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
    faceKeys: {},
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
export function stepEngine(st: EngineState, given: PatchV2, types: Record<string, ModuleType>, m: Music, read: (ref: SignalRef) => number, dt: number, opts: EngineOptions): EngineFrame {
  // A Music signal module is Music's port of its signal (music-signals.ts).
  const p = musicView(given);
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
  /** What a knob gives where the patch has no value for it: its type's default on a module it has; where it has no such
   *  module, its neutral, so it does nothing (what the rack shows is what runs, never a setting kept from before the
   *  switch: the owner, 2026-10-29); 0 for a type unknown. */
  const unset = (x: Module | undefined, type: string, knob: string) => {
    const k = (types[type] ?? ENGINE_TYPES[type])?.knobs.find((d) => d.id === knob);
    return (x ? k?.default : k?.neutral) ?? 0;
  };
  /** The chain's settings from its modules' knobs (the patch is the truth, 0057 Phase B; a knob it has no value for:
   *  `unset`). */
  // Intensity, Flash and Speed are the rack's macros on these knobs (0057: the Whole frame gone): as last frame left them.
  const macroedKnob = (x: Module | undefined, knob: string, or: number) => {
    return throughMacros(knobOf(x, knob, or), (x && plan.targetsOf.get(x.id)?.get(knob)) || [], (g) => finite(st.macros[g.macro.id], clamp(finite(g.macro.value), 0, 1)));
  };
  const chainSettings = {
    flash: macroedKnob(ofType('accent'), 'accent', unset(ofType('accent'), 'accent', 'accent')),
    speed: macroedKnob(ofType('clock'), 'speed', unset(ofType('clock'), 'clock', 'speed')),
    intensity: macroedKnob(ofType('envelope'), 'intensity', unset(ofType('envelope'), 'envelope', 'intensity')),
    continuity: CONTINUITY_AT.slice(0, BANDS).map((id) => knobOf(flameModule, id, unset(flameModule, 'flame', id))),
    evolution: EVOLUTION_AT.slice(0, BANDS).map((id) => knobOf(flameModule, id, unset(flameModule, 'flame', id))),
    fluid: knobOf(inChain('fluid'), 'fluid', unset(inChain('fluid'), 'fluid', 'fluid')),
    facets: knobOf(inChain('fluid'), 'facets', unset(inChain('fluid'), 'fluid', 'facets')),
    tunnel: knobOf(inChain('tunnel'), 'tunnel', unset(inChain('tunnel'), 'tunnel', 'tunnel')),
    dive: knobOf(inChain('dive'), 'dive', unset(inChain('dive'), 'dive', 'dive')),
    // Effects: the rack's macro on the effects' Amounts (folded into them: 1) where the Whole frame is gone.
    effects: knobOf(inChain('frame'), 'effects', inChain('frame') ? unset(inChain('frame'), 'frame', 'effects') : 1),
    smoothing: knobOf(inChain('show/blur'), 'smoothing', unset(inChain('show/blur'), 'blur', 'smoothing')),
    haze: knobOf(inChain('show/curve'), 'haze', unset(inChain('show/curve'), 'curve', 'haze')),
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
  if (opts.still) for (const s of HALVED) raw[T[s as keyof typeof T]] *= 0.5;
  raw[T.loudness] = env.level;
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
  // Each Grid laid out by its knobs as set and as their macros set them (seeded by its id: the same every frame and on
  // every device). Its macros at their value as last frame left them (the macros are worked out after the Grids, whose
  // channels the modulators read): a macro on Count or Spacing lays it out anew (0.10: they did nothing; the
  // owner, 2026-10-29: continuous over discrete).
  const points: Record<string, GridPoint[]> = {};
  const gridType = types.grid ?? ENGINE_TYPES.grid;
  for (const x of plan.grids) {
    const laid: Record<string, number> = { ...x.knobs };
    for (const k of gridType?.knobs ?? [])
      if (k.kind !== 'step' && plan.targetsOf.get(x.id)?.has(k.id))
        laid[k.id] = throughMacros(finite(x.knobs[k.id], k.default), plan.targetsOf.get(x.id)!.get(k.id)!, (g) => finite(st.macros[g.macro.id], clamp(finite(g.macro.value), 0, 1)), k.taper === 'exp');
    const key = JSON.stringify(laid);
    if (st.grids[x.id]?.key !== key) st.grids[x.id] = { key, grid: gridOf(laid, seedOf(x.id)) };
    points[x.id] = st.grids[x.id].grid.points;
    // Points cabled into At (a Flame's parts, where they are on the screen): the lattice shared out, a part laid round each.
    const at = gridAt(plan.intoOf.get(x.id)?.get('at')?.[0]?.from);
    if (at?.length) {
      const local = gridOf({ ...laid, count: Math.max(1, Math.round(points[x.id].length / at.length)) }, seedOf(x.id)).points;
      points[x.id] = at.flatMap((s) => local.map((q) => ({ x: s.x + q.x, y: s.y + q.y }))).slice(0, MOST_POINTS);
    }
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
    if (PROCESSORS.has(type)) return channelsOf(f.module)[channelled(type, f.module) ? (f.channel ?? 0) : 0];
    if (type === 'meter') return opts.meters ? finite(METER_OF[f.port]?.(opts.meters)) : 0;
    if (type === 'flame' && (f.port === 'x' || f.port === 'y' || f.port === 'size' || f.port === 'coverage')) return finite(flameFrameOf(f.module)?.[f.port]);
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
        return b !== undefined && f.channel !== undefined ? (bands[f.channel]?.[b] ?? 0) : 0;
      }
      return clamp(finite(read({ signal: r.signal, band: f.channel, reference: r.reference })) / 2, 0, 1);
    }
    if (type === 'envelope' && f.module === loudId) return f.port === 'out' ? env.out : env.level;
    if (type === 'clock') return f.port === 'beat' || f.port === 'bar' || f.port === 'phrase' ? ticks[f.port] : f.port === 'phase' ? st.clock.phase : (outputs['clock.rate'] ?? st.clock.rate);
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
  /** What scales a cable: its depth's value (none: nothing, 1). The loudness scaling is in sight now, the Loudness
   *  Follow's VCAs (0057 §7f; toV2), no normal. */
  const depthOf = (c: CableV2): number => {
    if (c.depth === null || c.depth === undefined) return 1;
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
      // A VCA is polyphonic only where something polyphonic reaches it (a follow VCA on one signal is one channel).
      const poly = (type === 'music' && BAND_PORTS.has(readingOf(c.from.port).signal)) || (type === 'adsr' && !trackAdsr(c.from.module, typeOf, into)) || (type === 'vca' ? plan.polyphonic.has(c.from.module) : CHANNELLED.has(type));
      return poly ? Array.from({ length: CHANNELS }, (_, ch) => ({ ...c, from: { ...c.from, channel: ch } })) : [c];
    });
  // The Screen's flash guard (WCAG 2.3.1; the limiter on the output, step 3): fixed, keyed by the flashes the
  // last frame made, a tenth at most held back; no knob in it.
  const guard =
    plan.first.has('screen') && opts.meters
      ? Math.max(0.1, stepDucker((st.duckers['screen|guard'] ??= newDucker()), 1, Math.max(opts.meters.flash, opts.meters.redFlash), FLASH_GUARD, step).gain)
      : 1;
  // The VCAs', Merges' and Duckers' channels: each one's buffer, last frame's until it is worked out this frame.
  const channelsOf = (id: string) => st.poly[id] ?? NO_CHANNELS;
  /** Whether a processor's output differs by channel (a VCA only where something polyphonic reaches it: else its one
   *  channel, whichever is asked). */
  const channelled = (type: string, id: string) => CHANNELLED.has(type) && (type !== 'vca' || plan.polyphonic.has(id));
  /** What a cable gives a port on channel `ch` (a knob's cable's rule: its curve at its source's x, times its depth
   *  and its walk); a VCA's or a Merge's source taken as it is, along its straight line where the value is a number. */
  // (A channel-less cable from a band's signal comes as its channel's, from the plan.)
  const giveOf = (entry: Entry, ch: number): number => {
    const { c, type } = entry;
    if (entry.v !== c.value) [entry.v, entry.shape] = [c.value, shapeOf(c.value)];
    const sh = entry.shape;
    if (PROCESSORS.has(type)) {
      const v = channelsOf(c.from.module)[channelled(type, c.from.module) ? (c.from.channel ?? ch) : 0];
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
      const ducked = stepDucker((st.duckers[x.id] ??= newDucker()), input, takes(x.id, 'key', 0), knobs, step);
      out.fill(ducked.out);
      // Its gain reduction (dB) for its face, as the Compressor gives one.
      outputs[(plan.faceKeys[x.id] ??= faceKeys(x.id)).reduction] = ducked.reduction;
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
        // Each channel in, for its face.
        outputs[(plan.faceKeys[x.id] ??= faceKeys(x.id)).in[ch]] = v;
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
  /** A step's Next fired: on to its next step, round past its last to its first (a pinned one stays as the store has
   *  it: the visuals' own changes never move a pinned knob). None with reduced motion (its "no moments"). */
  const nextOf = (module: string, id: string, k?: Pick<Knob, 'min' | 'max' | 'default'>) => {
    if (opts.still) return;
    const v = Math.round(finite(moduleOf.get(module)?.knobs[id], k?.default ?? 0));
    nexts.push({ module, knob: id, to: k && v + 1 > k.max ? k.min : v + 1 });
  };
  const modulesNow = plan.modulesNow;

  // The macros and the modulators, in the order their cables (and a modulator's macros) need.
  const knobs: Record<string, Record<string, number>> = {};
  for (const next of plan.order) {
    if (next.macro) {
      // As set, or as the macro one level up that maps it sets it; then as its cables move it.
      macros[next.id] = moved(next.id, MACRO_KNOB, macroed(next.id, MACRO_KNOB, clamp(finite(next.macro.value), 0, 1)), into.get(`${next.id}|value`) ?? []);
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
    // A Gate's input, for its face's scope.
    if (x.type === 'gate') outputs[(plan.faceKeys[x.id] ??= faceKeys(x.id)).input] = finite(k.in);
  }
  for (const id of Object.keys(st.modulators)) if (id in outputs) st.values[id] = outputs[id];
  for (let i = 0; i < plan.processorIds.length; i++) outputs[plan.outKeys[i]] = channelsOf(plan.processorIds[i])[0];
  outputs['screen.guard'] = guard;
  for (const id of Object.keys(macros)) st.macros[id] = macros[id];

  const n = BANDS;

  // Each chain module's own columns (step 5, the one engine): what its cables give it by the same rules, kept by the
  // module (any Flame, Tone map, Fluid…, wherever it sits, however many), and its knobs as its node reads them.
  if (!plan.owns) {
    plan.owns = [];
    const byId = new Map<string, NonNullable<typeof plan.owns>[number]>();
    for (const x of p.modules) {
      const fx = effectColumns(x.type);
      const columned = fx ? [...fx.keys()] : TYPE_KNOBS.get(x.type);
      if (!columned) continue;
      const columns = fx ?? new Map(columned.map((k) => [k, TYPE_COLUMN.get(`${x.type}|${k}`)!]));
      // An effect's controls swing about where they are set, as the chain's offsets do.
      const offsets = columned.filter((k) => (fx ? fx.get(k) !== A.fx1Amount : COLUMN_OFFSETS.has(`${x.type}.${k}`))).map((k) => ({ a: columns.get(k)!, knob: k }));
      const e = { own: { id: x.id, type: x.type, got: columnsFor(BANDS), columns, effect: !!fx }, cables: [], bands: [], gains: [], offsets };
      byId.set(x.id, e);
      plan.owns.push(e);
    }
    for (const [key, cables] of into) {
      const bar = key.indexOf('|');
      const e = byId.get(key.slice(0, bar));
      if (!e) continue;
      const port = key.slice(bar + 1);
      if (port.endsWith(NEXT)) continue;
      const gain = port.endsWith(GAIN);
      const knob = gain ? port.slice(0, -GAIN.length) : port;
      const a = e.own.columns.get(knob);
      if (a === undefined) continue;
      if (gain) e.gains.push({ a, knob });
      else {
        e.cables.push({ a, cables });
        if (!IS_LEVEL[a] && !IS_PART[a] && perChannel(cables).some((c) => c.from.channel !== undefined) && !e.bands.includes(a)) e.bands.push(a);
      }
    }
  }
  for (const { own, cables, bands: bandCols, gains, offsets } of plan.owns) {
    const g = own.got;
    g.whole.fill(0);
    g.level.fill(1);
    for (const b of g.band) b.fill(0);
    for (const { a, cables: list } of cables)
      for (const c of perChannel(list)) {
        const x = xOf(c);
        if (x === null) continue;
        const band = c.from.channel ?? null;
        if (band !== null && !(band >= 0 && band < n)) continue;
        const y = heightOf(shapeFor(c), x) * depthOf(c) * walkOf(c);
        if (IS_LEVEL[a]) g.level[a] += y / (band === null ? 1 : n);
        else if (band === null) g.whole[a] += y;
        else g.band[band][a] += y;
      }
    for (const a of bandCols) {
      let sum = 0;
      let most = -Infinity;
      for (let b = 0; b < n; b++) {
        sum += g.band[b][a];
        most = Math.max(most, g.band[b][a]);
      }
      g.whole[a] += IS_MOMENT[a] ? most : sum / n;
    }
    for (const { a, knob } of gains) {
      const k = gainOf(own.id, knob);
      if (IS_LEVEL[a]) g.level[a] *= k;
      else if (!IS_MOMENT[a]) {
        g.whole[a] *= k;
        for (const b of g.band) b[a] *= k;
      }
    }
    const x = moduleOf.get(own.id);
    for (const { a, knob } of offsets) g.whole[a] += own.effect ? baseOf(x!, knob, 0.5) : knobOf(x, knob, 0);
    knobs[own.id] = ownKnobs(own, x);
  }

  // The chain's own modules, each as its gain inputs hold it (the sidechains' Duckers, 0057 §7e): the Clock's speed
  // (its pace), the Accent's accent (the flash guard: its swing, and its courses slower), the Flame's evolving times
  // (a time's gain stretches it). Levers given (today's sidechaining, until the patch has its Duckers) act with them.
  const held: Levers = {
    ...L,
    pace: L.pace * gainOf(ofType('clock')?.id ?? 'clock', 'speed'),
    swing: L.swing * gainOf(ofType('accent')?.id ?? 'accent', 'accent'),
    evolution: L.evolution / Math.max(0.01, gainOf(flameModule?.id ?? 'flame', 'evolution')),
  };
  const clock = stepClock(st.clock, m.bps, m.playing, chainSettings.speed * (opts.still ? 0.25 : 1), held.pace, step);
  // The picture's level: the Tone map's Level knob as its gain input holds it (the pause's dimming, wired), where the
  // patch has the Tone map; else the Envelope's glow (today's).
  const tonemap = p.modules.find((x) => x.type === 'tonemap');
  const glow = tonemap ? finite(tonemap.knobs.level, 1) * gainOf(tonemap.id, 'level') : env.glow;
  outputs['clock.rate'] = clock;
  // Its Turn: where in the turn it is (0..1), the rate run on.
  st.clock.phase = (((st.clock.phase + (clock * step) / (2 * Math.PI)) % 1) + 1) % 1;
  outputs['clock.phase'] = st.clock.phase;

  // Each chain module's node values from its own knobs (step 5): what aspects.ts made of the chain's columns, its
  // Accent's courses its own.
  const params: Record<string, NodeParams> = {};
  const flames: Record<string, FlameView> = {};
  const follow = courseStep(step, held.swing);
  const accented = (id: string, knob: string, v: number, by = 1) => stepAccent((st.accents[`${id}|${knob}`] ??= newAccent()), v, follow, chainSettings.flash, held.swing, by);
  for (const { own } of plan.owns) {
    const k = knobs[own.id];
    const x = moduleOf.get(own.id);
    if (own.type === 'tonemap') {
      // Its Auto exposure: as bright however much of the view the Flame covers (against a third of it), as far as the knob.
      const coverage = flameFrameOf(flameModule?.id)?.coverage;
      const auto = coverage === undefined ? 1 : clamp(0.33 / Math.max(0.05, coverage), 0.4, 1.5);
      params[own.id] = { flash: (x ? finite(x.knobs.level, 1) * gainOf(own.id, 'level') : env.glow) * clamp(accented(own.id, 'glow', 1 + 0.5 * k.glow), 0.15, 3), exposure: 1 + (auto - 1) * finite(k.exposure, 1) };
    }
    else if (own.type === 'saturate') params[own.id] = { saturation: clamp(accented(own.id, 'saturation', 0.35 + 0.65 * k.saturation), 0, 1.25) };
    else if (own.type === 'curve') params[own.id] = hazeOf(k.haze);
    else if (own.type === 'blur') params[own.id] = { smoothing: 12 * k.smoothing ** 2 };
    else if (own.type === 'bend') params[own.id] = { waves: k.waves };
    else if (own.type === 'trails') params[own.id] = { keep: clamp(1 - 0.07 * 2 ** (-1.5 * k.trails), 0.5, 0.985) };
    else if (own.type === 'fluid') {
      const fluid = opts.fluidFull * k.fluid;
      params[own.id] = { fluid, warp: 0.12 * fluid, facets: k.facets };
    } else if (own.type === 'tunnel') params[own.id] = { tunnel: k.tunnel };
    else if (own.type === 'dive') params[own.id] = { dive: k.dive };
    else if (own.type === 'flame') {
      const continuity = Array.from({ length: BANDS }, (_, b) => knobOf(x, CONTINUITY_AT[b], unset(x, 'flame', CONTINUITY_AT[b])));
      const evolution = Array.from({ length: BANDS }, (_, b) => knobOf(x, EVOLUTION_AT[b], unset(x, 'flame', EVOLUTION_AT[b])));
      const flame = stepFlame((st.flames[own.id] ??= newFlame()), m, (knob, band) => (band === null ? k[knob] : k[`${knob}.${band}`]), { clock, continuity, evolution, accent: chainSettings.flash, still: opts.still }, step, held);
      const scale = clamp(accented(own.id, 'zoom', 0.5 + 0.5 * k.zoom, held.size), 0.25, 1.4);
      params[own.id] = { scale, flame };
      if (opts.view && x) {
        // Framed as far as its Fit; a patch from before the knob framed unless a Transform took its light.
        const fitSet = x.knobs.fit;
        const transformed = fitSet === undefined && p.cables.some((c) => c.from.module === own.id && c.from.port === 'light' && typeOf.get(c.to.module) === 'transform');
        const defaults = types[x.type]?.knobs;
        flames[own.id] = stepFlameNode((st.flameNodes[own.id] ??= newFlameNode()), {
          base: (knob) => baseOf(x, knob, defaults?.find((d) => d.id === knob)?.default ?? 0),
          fit: transformed ? 0 : finite(k.fit, finite(fitSet, 1)),
          now: flame,
          scale,
          at: opts.now,
          dt: step,
          cover: opts.view.cover,
          glide: opts.view.glide,
          aspect: finite(opts.aspect, 16 / 9),
          readsParts: plan.readsFlameParts,
        });
      }
    }
  }
  for (const id of Object.keys(st.accents)) if (!moduleOf.has(id.slice(0, id.indexOf('|')))) delete st.accents[id];
  for (const id of Object.keys(st.flames)) if (!moduleOf.has(id)) delete st.flames[id];
  for (const id of Object.keys(st.flameNodes)) if (!moduleOf.has(id)) delete st.flameNodes[id];

  // The Cratarr device's own values (its modules', each column what its module's is), for its stages, its Fluid's
  // stirring and the meters; none without a Flame.
  const dev = flameModule ? deviceOf() : null;

  // Every other module's knobs, as the evaluator moves a knob.
  if (!plan.others) {
    plan.others = [];
    for (const x of p.modules) {
      const t = types[x.type];
      if (!t || live.has(x.id) || PROCESSORS.has(x.type)) continue;
      if (plan.owns!.some((e) => e.own.id === x.id)) continue;
      plan.others.push({ x, t });
    }
  }
  for (const { x, t } of plan.others) if (!knobs[x.id]) knobs[x.id] = knobsOf(x, t);
  // Each Field: its time run on at its Speed.
  const fields: Record<string, { scale: number; phase: number; waves: number[]; shape: FieldShape }> = {};
  for (const x of p.modules) {
    if (x.type !== 'field') continue;
    const k = knobs[x.id] ?? x.knobs;
    const phase = (st.fieldPhases[x.id] = finite(st.fieldPhases[x.id], 0) + finite(k.speed, 0.25) * step);
    // Its Waves, a channel each: a band's cable each its band's, any other every wave's.
    const wk = types.field?.knobs.find((q) => q.id === 'waves');
    const into = plan.intoOf.get(x.id)?.get('waves') ?? NO_CABLES;
    let byWave = WAVE_CABLES.get(into);
    if (!byWave) WAVE_CABLES.set(into, (byWave = WAVES.map((_, ch) => perChannel(into).filter((c) => c.from.channel === undefined || c.from.channel === ch))));
    const waves = WAVES.map((_, ch) => {
      const base = finite(x.knobs[`waves.${ch}`], finite(x.knobs.waves, 1));
      return wk ? moved(x.id, { ...wk, id: `waves.${ch}` }, base, byWave[ch]) : base;
    });
    fields[x.id] = { scale: finite(k.scale, 1.2), phase, waves, shape: { range: finite(k.range, PLAIN.range), roughness: finite(k.roughness, PLAIN.roughness), variety: finite(k.variety, PLAIN.variety) } };
  }
  for (const id of Object.keys(st.fieldPhases)) if (!(id in fields)) delete st.fieldPhases[id];
  // Each Grid a Field is cabled into: its points carried along the flow at Drift, pulled back to its place at Return
  // (both as their cables move them), once the Field's time is run on.
  for (const x of plan.grids) {
    const g = st.grids[x.id];
    const from = plan.intoOf.get(x.id)?.get('field')?.[0]?.from.module;
    const field = from ? fields[from] : undefined;
    // A Field's flow, or the Fluid's as the renderer read it back.
    const flow = field ? (px: number, py: number) => flowAt(px, py, field.scale, field.phase, field.waves, field.shape) : from && typeOf.get(from) === 'fluid' ? opts.fluidFlow : undefined;
    if (!g || !flow) {
      if (g) g.offsets = undefined;
      continue;
    }
    const gridKnob = (id: string, or: number) => {
      const k = (types.grid ?? ENGINE_TYPES.grid).knobs.find((q) => q.id === id);
      return k ? moved(x.id, k, finite(x.knobs[id], k.default), plan.intoOf.get(x.id)?.get(id) ?? NO_CABLES) : finite(x.knobs[id], or);
    };
    const drift = gridKnob('drift', 0.4);
    const back = gridKnob('return', 0.6);
    const homes = points[x.id] ?? g.grid.points;
    const offsets = (g.offsets ??= homes.map((): [number, number] => [0, 0]));
    points[x.id] = homes.map((q, i) => {
      const o = (offsets[i] ??= [0, 0]);
      const [u, v] = flow(q.x + o[0], q.y + o[1]);
      o[0] += (u * drift - o[0] * back) * step;
      o[1] += (v * drift - o[1] * back) * step;
      return { x: q.x + o[0], y: q.y + o[1] };
    });
  }
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
    dice.push({ module: x.id, seed, scope: RANDOMIZE_SCOPES[clamp(Math.round(finite(x.knobs.scope, 0)), 0, RANDOMIZE_SCOPES.length - 1)], reach: clamp(finite(x.knobs.reach, 1), 0, 1) });
  }
  // What reaches each Fluid's `at`: the points output cabled into it (a Flame's, a Grid's), else the Flame's (its normal).
  const pointsOf = (r: PortRef): readonly (Point | null)[] | null => {
    const type = typeOf.get(r.module);
    if (type === 'flame' && r.port === 'points') return flamePointsOf(r.module);
    if (type === 'grid' && r.port === 'points') return (st.grids[r.module]?.grid.points ?? []).map((q): Point => [q.x * 0.5 + 0.5, q.y * 0.5 + 0.5]);
    return null;
  };
  const pointsAt: Record<string, readonly (Point | null)[]> = {};
  for (const x of plan.fluids) {
    const cabled = plan.intoOf.get(x.id)?.get('at')?.[0];
    const got = cabled ? pointsOf(cabled.from) : flameModule ? flamePointsOf(flameModule.id) : null;
    if (got) pointsAt[`${x.id}.at`] = got;
  }
  // The Cratarr device's Fluid stirred (stirring.ts; the frame loop's until 2026-10-29): its pushes at what reaches its
  // At (the Flame's parts by default), kicked by the bands' hits, the beats, downbeats and sections, each worth its own
  // Stir, Flow and Swirl (their macros in) times what the cables into them give.
  let fluid: EngineFrame['fluid'] = null;
  if (opts.view && dev && flameModule) {
    const fluidOwn = moduleOf.get(at + 'fluid');
    const fl = params[flameModule.id]?.flame;
    const bend = ofType('bend');
    const waves = bend ? (params[bend.id]?.waves ?? dev.given[A.waves]) : dev.given[A.waves];
    const gains = fluidGains(p);
    const beat = m.playing ? m.beat : null;
    const own = (aspect: 'stir' | 'flow' | 'swirl') => (fluidOwn ? baseOf(fluidOwn, aspect, unset(fluidOwn, 'fluid', aspect)) : unset(undefined, 'fluid', aspect));
    fluid = stir(st.stirring, {
      t: opts.now / 1000,
      dt: step,
      amount: dev.fluid,
      parts: ((fluidOwn && pointsAt[`${fluidOwn.id}.at`]) ?? flamePointsOf(flameModule.id) ?? []) as ([number, number] | null)[],
      turns: fl?.rates ?? [],
      lights: fl?.hits ?? [],
      level: dev.bands.map((v) => v[B.level]),
      continuity: Array.from({ length: BANDS }, (_, b) => baseOf(flameModule, CONTINUITY_AT[b], unset(flameModule, 'flame', CONTINUITY_AT[b]))),
      hits: m.playing && m.hits ? m.hits.map((hh) => (hh.index < 0 ? null : { index: hh.index, strength: hh.weight, nextIn: hh.nextIn })) : [],
      beat: beat ? { ...beat, nextIn: beat.length - beat.since } : null,
      downbeat: dev.track[T.downbeat],
      section: dev.track[T.section],
      loudness: dev.lit,
      swell: dev.track[T.swell],
      bps: m.bps,
      waves,
      position: m.position,
      center: [0.5, 0.5],
      gain: (signal, band, aspect, x) => own(aspect as 'stir' | 'flow' | 'swirl') * gains(signal, band, aspect as 'stir', x),
    });
    // Its Fire (0.9.18): Lift, Open walls and Cooling as set (their macros in); none on a Fluid without them.
    if (fluidOwn) for (const k of ['lift', 'open', 'cooling'] as const) fluid.flow[k] = baseOf(fluidOwn, k, unset(fluidOwn, 'fluid', k));
  }
  // A Flame's points in a Grid's frame (Streamlines' seeds, Instance's): its parts on the screen, where it has a view.
  for (const id of Object.keys(flames)) {
    const seeds = gridAt({ module: id, port: 'points' });
    if (seeds) points[id] = seeds;
  }
  // The stages' values (the renderer's), from the device's: the light loop and the camera at the levers' pace, the effects
  // as their slots set them (the frame loop's chainNow until 0.10).
  let device: DeviceFrame | null = null;
  if (dev && flameModule) {
    const first = (type: string) => given.modules.find((x) => x.type === type);
    const how = { ...effectSlotsOf(given), lattice: asSet(given, first('fluid'), 'lattice', types) ?? 0 };
    const speed = asSet(given, first('clock'), 'speed', types) ?? types.clock?.knobs.find((k) => k.id === 'speed')?.default ?? 1;
    const stirred = fluid ?? { forces: [], flow: { energy: 0, swirl: 0 } };
    const pace = L.pace;
    const bend = first('bend');
    device = {
      stages: {
        points: { turn: dev.turn, scale: dev.scale, dive: DIVE_MOST * dev.dive * pace },
        carry: {
          keep: dev.keep,
          fluid: dev.fluid,
          tunnel: TUNNEL_MOST * dev.tunnel * pace,
          facets: { amount: dev.facets, hex: Math.round(how.lattice) === 1 },
          forces: stirred.forces,
          flow: stirred.flow,
          // At the Speed setting's pace, as the turning; the fluid takes at most 0.05 s a step.
          dt: dt * speed * pace,
          warp: dev.warp,
        },
        show: {
          brightness: 1,
          keep: dev.keep,
          gamma: dev.gamma,
          vibrancy: 1,
          zoom: 1,
          flash: dev.flash,
          hits: dev.hits,
          ceiling: opts.ceiling ?? 1,
          saturation: dev.saturation,
          exposure: 1,
          smoothing: dev.smoothing,
          floor: dev.floor,
          fire: flames[flameModule.id]?.fire ?? 0,
          flow: stirred.flow,
        },
        effects: effectsNow(how, dev.effects, m.position, m.playing ? m.beat : null, L, dev.routed),
      },
      meter: {
        waves: (bend ? (params[bend.id]?.waves ?? dev.given[A.waves]) : dev.given[A.waves]) * pace,
        fluid: dev.fluid,
        tunnel: dev.tunnel * pace,
        dive: dev.dive * pace,
        effects: settingsReport(how, dev.effects, L, dev.routed),
        symmetry: dev.symmetry,
      },
    };
  }
  return { device, knobs, params, outputs, macros, nexts, fields, points, pointsAt, dice, flames, fluid };

  /** A module's knobs as its macros and cables move them. */
  /** A knob's base as its macros set it, along its taper (an exponential knob's geometrically: 0057's Wild). */
  function macroed(module: string, knob: Knob | undefined, base: number): number {
    return throughMacros(base, (knob && plan.targetsOf.get(module)?.get(knob.id)) || [], (g) => macroOf(g.macro), knob?.taper === 'exp');
  }
  /** A Flame's parts on the screen and its outputs: its node's, as last found, where the engine runs with a view; else
   *  as the caller gives them (a lab, a test). */
  function flamePointsOf(id: string | undefined): readonly (Point | null)[] | null {
    if (opts.view) return (id && st.flameNodes[id]?.parts.length ? st.flameNodes[id].parts : null) ?? null;
    return opts.flamePoints ?? null;
  }
  function flameFrameOf(id: string | undefined) {
    return opts.view ? (id ? st.flameNodes[id]?.frame : null) : opts.flameFrame;
  }
  /** An engine module's knob's base: as set, its macros' (the Ducker's threshold under Wild). */
  function baseOf(x: Module, id: string, or: number) {
    return macroed(x.id, (types[x.type] ?? ENGINE_TYPES[x.type])?.knobs.find((k) => k.id === id) ?? ({ id } as Knob), finite(x.knobs[id], or));
  }

  /** What a Grid's At takes: a Flame's parts where they are on the screen, in the Grid's frame (centred, the short side
   *  -1..1); none for anything else. */
  function gridAt(from: PortRef | undefined): GridPoint[] | null {
    const parts = from && typeOf.get(from.module) === 'flame' && from.port === 'points' ? flamePointsOf(from.module) : null;
    if (!parts) return null;
    const a = finite(opts.aspect, 16 / 9);
    return parts.filter((q): q is Point => !!q).map(([px, py]) => (a > 1 ? { x: (2 * px - 1) * a, y: 2 * py - 1 } : { x: 2 * px - 1, y: (2 * py - 1) / a }));
  }

  /** A module's step knobs its type declares, past its columns' moments. */
  function stepsOf(own: Own): Knob[] {
    return ((types[own.type] ?? ENGINE_TYPES[own.type])?.knobs ?? []).filter((k) => k.kind === 'step' && !own.columns.has(k.id));
  }

  /** The Cratarr device's own values, from its modules' own columns and values. */
  function deviceOf() {
    const ownOf = new Map(plan.owns!.map((e) => [e.own.id, e.own]));
    const W = new Float32Array(NA);
    const level = new Float32Array(NA).fill(1);
    // Each column its module's: the device's (beside its Flame), the effects' slots' (their routes only: no setting).
    for (const [key, a] of COLUMN) {
      const bar = key.indexOf('|');
      const own = ownOf.get(at + key.slice(0, bar));
      if (!own) continue;
      W[a] = own.got.whole[a];
      level[a] = own.got.level[a];
    }
    const slots = slotModules(p.modules);
    SLOT_IDS.forEach((slot, i) => {
      const x = slots.get(slot);
      const own = x && ownOf.get(x.id);
      if (!own || !x) return;
      level[A[AMOUNT_ASPECTS[i]]] = own.got.level[A.fx1Amount];
      for (const [j, k] of (['k1', 'k2'] as const).entries()) {
        const name = ownKnob(x.type, k);
        if (name) W[A[KNOB_ASPECTS[i * 2 + j]]] = own.got.whole[j ? A.fx1K2 : A.fx1K1] - baseOf(x, name, 0.5);
      }
    });
    const of = (type: string, rel: string) => (moduleOf.get(at + rel)?.type === type ? params[at + rel] : undefined);
    const flame = params[flameModule!.id]!;
    const tone = of('tonemap', 'show/tonemap');
    const sat = of('saturate', 'show/saturate');
    const curve = of('curve', 'show/curve');
    const blur = of('blur', 'show/blur');
    const fluidOwn = of('fluid', 'fluid');
    const fluid = fluidOwn?.fluid ?? opts.fluidFull * clamp(chainSettings.fluid, 0, 1);
    const haze = curve?.gamma !== undefined ? { gamma: curve.gamma, floor: curve.floor ?? 0 } : hazeOf(clamp(chainSettings.haze, -1, 1));
    const moments = Object.fromEntries(MOMENT_ASPECTS.map((a) => [a, false])) as Record<(typeof MOMENT_ASPECTS)[number], boolean>;
    for (const r of nexts) {
      const own = ownOf.get(r.module);
      if (!own) continue;
      const slot = slotOf(r.module, own.type);
      const a = own.columns.get(r.knob) ?? (slot && r.knob === 'effect' ? A[SLOT_MOMENT[slot]] : undefined);
      if (a !== undefined && IS_MOMENT[a] && (r.module.startsWith(at) || slot)) moments[ASPECTS[a] as (typeof MOMENT_ASPECTS)[number]] = true;
    }
    return {
      given: W,
      track: raw,
      bands,
      lit: env.level,
      glow,
      turn: flame.flame!.turn,
      rates: flame.flame!.rates,
      shape: flame.flame!.shape,
      parts: flame.flame!.parts,
      hits: flame.flame!.hits,
      scale: flame.scale!,
      keep: of('trails', 'trails')?.keep ?? clamp(1 - 0.07, 0.5, 0.985),
      gamma: haze.gamma,
      flash: tone?.flash ?? glow * clamp(accented(`${flameModule!.id}|chain`, 'glow', 1), 0.15, 3),
      saturation: sat?.saturation ?? clamp(accented(`${flameModule!.id}|chain`, 'saturation', 0.35), 0, 1.25),
      smoothing: blur?.smoothing ?? 12 * clamp(chainSettings.smoothing, 0, 1) ** 2,
      floor: haze.floor,
      fluid,
      warp: fluidOwn?.warp ?? 0.12 * fluid,
      tunnel: of('tunnel', 'tunnel')?.tunnel ?? clamp(chainSettings.tunnel, 0, 1),
      effects: clamp(chainSettings.effects * level[A.effects], 0, 1),
      facets: fluidOwn?.facets ?? clamp(chainSettings.facets, 0, 1),
      dive: of('dive', 'dive')?.dive ?? clamp(chainSettings.dive, 0, 1),
      symmetry: moments.symmetry,
      routed: { amounts: AMOUNT_ASPECTS.map((a) => level[A[a]]), knobs: KNOB_ASPECTS.map((a) => clamp(W[A[a]], -1, 1)) },
    };
  }

  /** A chain module's knobs as its node reads them (step 5), from its own columns: a swing its column (a part's per band
   *  too, `<knob>.<band>`), a level its setting times its column's level, within its range (Haze −1..1); its other knobs
   *  as set (the Flame's continuity and evolution per band). */
  function ownKnobs(own: Own, x: Module | undefined): Record<string, number> {
    const k: Record<string, number> = { ...x?.knobs };
    const declared = (types[own.type] ?? ENGINE_TYPES[own.type])?.knobs;
    const g = own.got;
    for (const [id, a] of own.columns) {
      // A moment (a Flame's family, form…) moves on as its Next fires.
      if (IS_MOMENT[a]) {
        if (fired(own.id, id)) nextOf(own.id, id, declared?.find((x) => x.id === id));
        continue;
      }
      if (IS_LEVEL[a]) {
        const base = own.effect && x ? baseOf(x, id, 0) : finite(x?.knobs[id], declared?.find((d) => d.id === id)?.default ?? 0);
        k[id] = clamp(base * g.level[a], a === A.haze ? -1 : 0, 1);
        continue;
      }
      k[id] = own.effect ? clamp(g.whole[a], 0, 1) : g.whole[a];
      if (IS_PART[a]) for (let b = 0; b < g.band.length; b++) k[`${id}.${b}`] = g.whole[a] + g.band[b][a];
    }
    // Its steps (a Flame's family, an engine's effect…) move on as their Next fires, as any module's do; its other
    // knobs (a Flame's Fit, Tone map's Level and Auto exposure) as any module's are, by their macros and cables.
    for (const knob of stepsOf(own)) if (x) k[knob.id] = moved(own.id, knob, finite(x.knobs[knob.id], knob.default), NO_CABLES);
    for (const knob of declared ?? [])
      if (x && knob.kind !== 'step' && !own.columns.has(knob.id) && !/\.\d+$/.test(knob.id)) k[knob.id] = moved(own.id, knob, macroed(own.id, knob, finite(x.knobs[knob.id], knob.default)), plan.intoOf.get(own.id)?.get(knob.id) ?? NO_CABLES);
    return k;
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
      if (fired(module, k.id)) nextOf(module, k.id, k);
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
