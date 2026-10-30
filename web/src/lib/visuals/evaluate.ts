// The patch's evaluator (decision 0056, step 4): each frame, every knob's value as its macros and cables move it, from
// the music's signals and the modulators, for the modules the patch adds (new sources, modules on a layer, the
// picture's loop, new finish modules, the modulators). Today's pipeline keeps aspects.ts (`TODAYS_IDS`).
//
// - A signal comes in as x = clamp(s / 2, 0, 1): 1 = typical is half way. A modulator's output comes in as it is
//   (0..1); a bipolar one's as (v + 1) / 2, so its rest is half way as a typical signal's is.
// - What a cable gives (y) is its curve's height at x, a straight one's amount times x, as routes.ts has it; walking,
//   times its walk factor (`opts.walk`, by cable id: Behaviour's walk, as `give`'s `walk`).
// - A swing moves its knob by e·y·w each, times half the knob's range; a level lets through `letThrough` of its
//   setting, the cables multiplying (routes.ts). The envelope (e) scales every cable but the loudness's own.
// - Band cables into a knob that is not a band's own (`name.<band>`): the swings their mean over the bands, the levels
//   their geometric mean, as `give` takes them (over every band, the ones with no cable giving 0 and 1).
// - A step is not moved: where what its cables give rises past a half it asks for a moment (`moments`; morph.ts dips
//   its module through neutral to take it), ready again under a third and no sooner than `MOMENT_GAP` (aspects.ts).
// - A modulator's port sums its cables on its source's own scale (a signal's 1 = typical, a modulator's output as it
//   comes in), not scaled by the envelope: a gate's Above 1 is "above typical". A straight cable passes its signal
//   times its amount; a curved one, its curve's height at x times 2 for a signal (back on the signal's scale).
// - Beats into a port are counted exactly: given the frame (`opts.frame`), a cable from 'beat' carries 1 on the frame
//   its `beat.index` moves on (not on the first frame seen), 0 otherwise, in place of the normalised pulse, which can
//   miss a weak beat. FrameInfo has no bar index yet, so 'downbeat' stays the pulse. A gate fed it needs Above under 1.
// - The 'track' signal is aspects.ts's own (Music leaves it at 0): nothing here gives it.
// - Macros and modulators go in the order their cables need; in a cycle the first left reads last frame's value.
// - A macro is a knob (`{module: macro.id, knob: 'value'}`, 0..1): its value sets each target's base to
//   lerp(from, to, value) before the cables act (macros in order, the last one's where two reach one knob; not steps).
//
// Pure: `EvalState` is the caller's, kept from frame to frame.

import { curveFrom, heightAt, isStraight, tableOf, type Curve, type RouteValue } from './curves';
import { seedOf } from './flame';
import { throughMacros } from './macro-map';
import { BANDS, modulatorOf, type ModulatorType } from './modulators/modulators';
import { modulesOf, type Cable, type FrameInfo, type Knob, type Macro, type Module, type ModuleType, type Modulator, type Patch, type SignalRef } from './patch';

/** Today's pipeline's module types (patch.ts's Cratarr chain), which aspects.ts moves. */
/** Today's pipeline's modules (patch.ts's Cratarr chain), by id: aspects.ts moves them. By id, not type: an effect
 *  engine on a layer has the finish slots' type, and is the patch's to move (it was skipped once). */
export const TODAYS_IDS: ReadonlySet<string> = new Set(['frame', 'flame', 'show', 'fluid', 'tunnel', 'dive', 'trails', 'loop-effect', 'fx1', 'fx2', 'fx3']);

/** A moment comes again no sooner than this (seconds; aspects.ts's 2000 ms). */
export const MOMENT_GAP = 2;

/** What the caller keeps: each modulator running and its last output, each macro's last value, each step's moment. */
export type EvalState = {
  modulators: Record<string, { type: string; run: Modulator }>;
  values: Record<string, number>;
  macros: Record<string, number>;
  moments: Record<string, { armed: boolean; since: number }>;
  /** The beat index last seen (undefined: none yet; null: none playing). */
  beat?: number | null;
};

export const newEvalState = (): EvalState => ({ modulators: {}, values: {}, macros: {}, moments: {} });

export type EvalOptions = {
  /** The loudness as heard (0..1): it scales every cable but the loudness's own. */
  envelope: number;
  /** Each walking cable's factor now, by cable id (1: as set). */
  walk?: Record<string, number>;
  /** The bands a track's signals have (6). */
  bands?: number;
  /** The modules left to aspects.ts (by default today's pipeline's). */
  skip?: (m: Module) => boolean;
  /** The frame's beat (patch.ts FrameInfo), so beats into a port count exactly; without it, the normalised pulse. */
  frame?: Pick<FrameInfo, 'beat'>;
};

/** A step that asks to move on: morph.ts dips its module through neutral to take it. */
export type MomentRequest = { module: string; knob: string };

export type Evaluated = {
  /** Each module's knobs this frame (a modulator's with its ports' sums). */
  knobs: Record<string, Record<string, number>>;
  /** Each modulator's output this frame (its own range). */
  outputs: Record<string, number>;
  /** Each macro's value this frame. */
  macros: Record<string, number>;
  moments: MomentRequest[];
};

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const finite = (v: unknown, or = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : or);

/** A level's cable's share let through; walking (`w` under 1), its pull as much less (routes.ts, unexported there). */
export const letThrough = (peak: number, y: number, e: number, w = 1) => 1 - w * (1 - Math.min(1, Math.max(0, 1 - e * (peak - y))));

/** A cable's value as `compile` takes it: its end, its table where it bends, its peak, and whether it walks. */
export type Shape = { end: number; table: Float32Array | null; peak: number; walks: boolean };
const shapes = new WeakMap<Curve, { key: string; shape: Shape }>();
export function shapeOf(v: RouteValue): Shape {
  if (typeof v === 'number') {
    const end = finite(v);
    return { end, table: null, peak: Math.max(0, end), walks: false };
  }
  const key = JSON.stringify(v);
  const kept = shapes.get(v);
  if (kept?.key === key) return kept.shape;
  const c = curveFrom(v);
  const bent = c && !isStraight(c) ? tableOf(c) : null;
  const end = c ? c.h : 0;
  const shape = { end, table: bent ? bent.table : null, peak: bent ? bent.peak : Math.max(0, end), walks: !!c?.walks };
  shapes.set(v, { key, shape });
  return shape;
}
export const heightOf = (s: Shape, x: number) => (s.table ? heightAt(s.table, x) : s.end * x);

/** A knob that is one band's own (`lighting.2`): band cables into it act as the whole track's do. */
const BAND_KNOB = /\.\d+$/;

/** A continuous knob moved from `base` by its cables (the rules above; a step is not moved here): `x` each cable's x
 *  (null: its source is not there), `walk` its walk factor, `envelope` 0..1, `n` the bands. Also chain-cables.ts's,
 *  for today's knobs that are no routes' column. */
export function moveKnob(k: Knob, base: number, cables: readonly Cable[], x: (c: Cable) => number | null, walk: (c: Cable) => number, envelope: number, n: number): number {
  const ownBand = BAND_KNOB.test(k.id);
  let swing = 0;
  let level = 1;
  let bandSwing = 0;
  let bandLevel = 1;
  for (const c of cables) {
    const at = x(c);
    if (at === null) continue;
    const band = 'signal' in c.from && c.from.band !== undefined ? c.from.band : null;
    if (band !== null && !(Number.isInteger(band) && band >= 0 && band < n)) continue;
    const s = shapeOf(c.value);
    const y = heightOf(s, at);
    const w = s.walks ? walk(c) : 1;
    const e = 'signal' in c.from && c.from.signal === 'loudness' && band === null ? 1 : envelope;
    const apart = band !== null && !ownBand;
    if (c.mode === 'level') {
      const through = letThrough(s.peak, y, e, w);
      if (apart) bandLevel *= through;
      else level *= through;
    } else if (apart) bandSwing += e * y * w;
    else swing += e * y * w;
  }
  const v = base * level * (n ? bandLevel ** (1 / n) : 1) + (swing + (n ? bandSwing / n : 0)) * ((k.max - k.min) / 2);
  return clamp(v, k.min, k.max);
}
const MACRO_KNOB: Knob = { id: 'value', label: 'Value', min: 0, max: 1, neutral: 0, default: 0, kind: 'swing' };
const at = (module: string, knob: string) => `${module}|${knob}`;

function modulatorType(types: Record<string, ModuleType>, type: string): ModulatorType | null {
  const t = types[type] as Partial<ModulatorType> | undefined;
  if (t && typeof t.create === 'function') return t as ModulatorType;
  return modulatorOf(type) ?? null;
}

/** One frame: each module's knobs as its macros and cables move them, the modulators stepped `dt` seconds on. */
export function evaluate(patch: Patch, types: Record<string, ModuleType>, signals: (ref: SignalRef) => number, state: EvalState, dt: number, opts: EvalOptions): Evaluated {
  const n = opts.bands ?? BANDS;
  const skip = opts.skip ?? ((m: Module) => TODAYS_IDS.has(m.id));
  const step = Number.isFinite(dt) ? Math.max(0, dt) : 0;
  const envelope = clamp(finite(opts.envelope, 1), 0, 1);
  const mods = modulesOf(patch);
  const index = opts.frame ? (opts.frame.beat?.index ?? null) : undefined;
  const onBeat = index !== undefined && index !== null && state.beat !== undefined && index !== state.beat ? 1 : 0;
  if (index !== undefined) state.beat = index;

  // The modulators running: one each, anew where its type changed, none for those gone.
  const live = new Set(patch.modulators.map((m) => m.id));
  for (const id of Object.keys(state.modulators)) {
    if (!live.has(id)) {
      delete state.modulators[id];
      delete state.values[id];
    }
  }
  const bipolar = new Set<string>();
  for (const m of patch.modulators) {
    const t = modulatorType(types, m.type);
    if (!t) {
      delete state.modulators[m.id];
      delete state.values[m.id];
      continue;
    }
    if (t.range === 'bipolar') bipolar.add(m.id);
    if (state.modulators[m.id]?.type !== m.type) {
      state.modulators[m.id] = { type: m.type, run: t.create(seedOf(m.id)) };
      state.values[m.id] = 0;
    }
  }
  const macroIds = new Set(patch.macros.map((x) => x.id));
  for (const id of Object.keys(state.macros)) if (!macroIds.has(id)) delete state.macros[id];
  for (const key of Object.keys(state.moments)) if (!mods.has(key.slice(0, key.indexOf('|')))) delete state.moments[key];

  const into = new Map<string, Cable[]>();
  for (const c of patch.cables) {
    const key = at(c.to.module, c.to.knob);
    (into.get(key) ?? into.set(key, []).get(key)!).push(c);
  }
  const targets = new Map<string, { macro: Macro; from: number; to: number }[]>();
  for (const macro of patch.macros)
    for (const t of macro.targets) {
      const key = at(t.module, t.knob);
      (targets.get(key) ?? targets.set(key, []).get(key)!).push({ macro, from: t.from, to: t.to });
    }

  const outputs: Record<string, number> = {};
  const macros: Record<string, number> = {};
  const moments: MomentRequest[] = [];
  const outputOf = (id: string) => (id in outputs ? outputs[id] : finite(state.values[id]));
  const macroOf = (m: Macro) => (m.id in macros ? macros[m.id] : finite(state.macros[m.id], clamp(finite(m.value), 0, 1)));
  const walkOf = (c: Cable, s: Shape) => (s.walks ? finite(opts.walk?.[c.id], 1) : 1);
  /** A cable's x (0..1), or null where its modulator is not running. */
  const xOf = (c: Cable): number | null => {
    if ('modulator' in c.from) {
      const id = c.from.modulator;
      if (!state.modulators[id]) return null;
      const v = outputOf(id);
      return clamp(bipolar.has(id) ? (v + 1) / 2 : v, 0, 1);
    }
    return clamp(finite(signals(c.from)) / 2, 0, 1);
  };

  /** A knob's value from its base, as its cables move it; a step's moment asked for. */
  function moved(module: string, k: Knob, base: number): number {
    const key = at(module, k.id);
    const cables = into.get(key);
    if (!cables) return clamp(base, k.min, k.max);
    if (k.kind !== 'step') return moveKnob(k, base, cables, xOf, (c) => walkOf(c, shapeOf(c.value)), envelope, n);
    const ownBand = BAND_KNOB.test(k.id);
    let swing = 0;
    let level = 1;
    let bandSwing = 0;
    let bandLevel = 1;
    const bandSums = k.kind === 'step' ? new Float64Array(n) : null;
    for (const c of cables) {
      const x = xOf(c);
      if (x === null) continue;
      const band = 'signal' in c.from && c.from.band !== undefined ? c.from.band : null;
      if (band !== null && !(Number.isInteger(band) && band >= 0 && band < n)) continue;
      const s = shapeOf(c.value);
      const y = heightOf(s, x);
      const w = walkOf(c, s);
      const e = 'signal' in c.from && c.from.signal === 'loudness' && band === null ? 1 : envelope;
      const apart = band !== null && !ownBand;
      if (c.mode === 'level' && k.kind !== 'step') {
        const through = letThrough(s.peak, y, e, w);
        if (apart) bandLevel *= through;
        else level *= through;
      } else if (apart) {
        bandSwing += e * y * w;
        if (bandSums) bandSums[band] += e * y * w;
      } else swing += e * y * w;
    }
    if (bandSums) {
      // The step's moments: what its cables give (the bands' largest, as `give`'s moments), rising past a half.
      const given = swing + (n ? Math.max(...bandSums) : 0);
      const mo = (state.moments[key] ??= { armed: true, since: Infinity });
      mo.since += step;
      if (given < 1 / 3) mo.armed = true;
      if (given > 0.5 && mo.armed && mo.since >= MOMENT_GAP) {
        mo.armed = false;
        mo.since = 0;
        moments.push({ module, knob: k.id });
      }
      return clamp(base, k.min, k.max);
    }
    const v = base * level * (n ? bandLevel ** (1 / n) : 1) + (swing + (n ? bandSwing / n : 0)) * ((k.max - k.min) / 2);
    return clamp(v, k.min, k.max);
  }

  /** A modulator's port: its cables summed on their sources' own scales. */
  function portOf(module: string, port: string): number {
    let sum = 0;
    for (const c of into.get(at(module, port)) ?? []) {
      const s = shapeOf(c.value);
      const w = walkOf(c, s);
      if ('modulator' in c.from) {
        const x = xOf(c);
        if (x !== null) sum += heightOf(s, x) * w;
      } else {
        const exact = index !== undefined && c.from.signal === 'beat' && c.from.band === undefined;
        const raw = exact ? onBeat : finite(signals(c.from));
        sum += (s.table ? 2 * heightAt(s.table, clamp(raw / 2, 0, 1)) : s.end * raw) * w;
      }
    }
    return sum;
  }

  function knobsOf(m: Module, t: ModuleType | undefined, ports: string[] = []): Record<string, number> {
    const out: Record<string, number> = { ...m.knobs };
    for (const k of t?.knobs ?? []) {
      let base = finite(m.knobs[k.id], k.default);
      if (k.kind !== 'step') base = throughMacros(base, targets.get(at(m.id, k.id)) ?? [], (x) => macroOf(x.macro));
      out[k.id] = moved(m.id, k, base);
    }
    for (const p of ports) out[p] = portOf(m.id, p);
    return out;
  }

  // The order the macros and the modulators need: each after what its cables (and, a modulator, its macros) read.
  type Node = { id: string; macro?: Macro; module?: Module };
  const nodes: Node[] = [...patch.macros.map((macro) => ({ id: macro.id, macro })), ...patch.modulators.filter((m) => state.modulators[m.id]).map((module) => ({ id: module.id, module }))];
  const ids = new Set(nodes.map((x) => x.id));
  const needs = new Map<string, Set<string>>(nodes.map((x) => [x.id, new Set<string>()]));
  for (const c of patch.cables) if ('modulator' in c.from && ids.has(c.from.modulator) && ids.has(c.to.module)) needs.get(c.to.module)!.add(c.from.modulator);
  for (const macro of patch.macros) for (const t of macro.targets) if (ids.has(t.module) && !macroIds.has(t.module)) needs.get(t.module)!.add(macro.id);
  const done = new Set<string>();
  const result: Evaluated = { knobs: {}, outputs, macros, moments };
  while (done.size < nodes.length) {
    const next = nodes.find((x) => !done.has(x.id) && [...needs.get(x.id)!].every((d) => done.has(d))) ?? nodes.find((x) => !done.has(x.id))!;
    done.add(next.id);
    if (next.macro) {
      macros[next.id] = moved(next.id, MACRO_KNOB, clamp(finite(next.macro.value), 0, 1));
      continue;
    }
    const m = next.module!;
    const t = modulatorType(types, m.type)!;
    const knobs = knobsOf(m, t, t.inputs ?? []);
    result.knobs[m.id] = knobs;
    outputs[m.id] = state.modulators[m.id].run.step(step, signals, knobs);
  }
  for (const m of mods.values()) if (!live.has(m.id) && !skip(m)) result.knobs[m.id] = knobsOf(m, types[m.type]);
  for (const id of Object.keys(outputs)) state.values[id] = outputs[id];
  for (const id of Object.keys(macros)) state.macros[id] = macros[id];
  return result;
}
