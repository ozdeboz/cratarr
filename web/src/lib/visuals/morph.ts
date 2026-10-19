// The morph (decision 0056, step 6): from one state to another knob by knob over a time, never a cut.
//
// - Each continuous knob goes from where it is shown to its new value, eased (smoothstep) over the morph's length.
// - A chain change: a module both chains have (the same id and type) carries its state over and morphs; one only the
//   new chain has fades in from its neutral, one only the old has fades out to it and is dropped there.
// - A step (symmetry order, an effect's choice, the lattice) changes only while its module is at neutral: its module's
//   level knobs dip to their neutral over half a beat (every continuous knob of a module with no level), the step
//   switches there, and they come back over the next half. A morph's own step changes and evaluate.ts's moments
//   (`askStep`) both go through it. A module fading in takes its steps at once: it starts at neutral.
// - The device's knobs (Quality, FPS, brightness) are never morphed: their new value at once.
// - Half a beat: `beat`, the seconds a beat lasts, from the tempo; without one 90 BPM, as aspects.ts turns without one.
//
// What it moves is the state (the patch's knobs as set), which evaluate.ts then moves by the cables. Pure and dt-based:
// the same at 30 and 120 frames a second. `Morph` is the caller's, kept from frame to frame.

import type { Knob, Module, ModuleType } from './patch';

/** A beat's length where the tempo is not known (seconds: 90 BPM). */
export const FALLBACK_BEAT = 60 / 90;

type Entry = {
  id: string;
  type: string;
  from: Record<string, number>;
  to: Record<string, number>;
  /** Its steps as shown (a step's new value waits in its dip). */
  steps: Record<string, number>;
  leaving: boolean;
};

/** A module's dip through neutral: how deep (0..1, linear in time), going down or back, the steps to take at the
 *  bottom. */
type Dip = { depth: number; down: boolean; pending: Record<string, number> };

export type Morph = {
  /** The new chain's modules in its order, then the old one's still fading out. */
  entries: Entry[];
  elapsed: number;
  length: number;
  /** By module id (the new chain's). */
  dips: Record<string, Dip>;
};

/** A module fading out, as shown this frame. */
export type Leaving = { id: string; type: string; knobs: Record<string, number> };

export type MorphFrame = {
  /** The new chain's modules' knobs this frame, by module id. */
  knobs: Record<string, Record<string, number>>;
  /** The old chain's modules still fading out. */
  leaving: Leaving[];
  /** Nothing moving: the morph done, no dip under way. */
  still: boolean;
};

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const ease = (t: number) => t * t * (3 - 2 * t);
const EPS = 1e-9;

const knobsOf = (types: Record<string, ModuleType>, type: string): Knob[] => types[type]?.knobs ?? [];
const isStep = (k: Knob | undefined) => k?.kind === 'step';
const neutralOf = (knobs: Knob[], values: Record<string, number>) => {
  const out: Record<string, number> = { ...values };
  for (const k of knobs) if (!k.device) out[k.id] = k.neutral;
  return out;
};
const stepsOf = (knobs: Knob[], values: Record<string, number>) => Object.fromEntries(knobs.filter(isStep).map((k) => [k.id, values[k.id] ?? k.default]));

/** At rest at these modules' values. */
export function newMorph(modules: Module[], types: Record<string, ModuleType>): Morph {
  return {
    entries: modules.map((m) => ({ id: m.id, type: m.type, from: { ...m.knobs }, to: { ...m.knobs }, steps: stepsOf(knobsOf(types, m.type), m.knobs), leaving: false })),
    elapsed: 0,
    length: 0,
    dips: {},
  };
}

/** Each continuous knob of an entry where the morph has it now (no dip). */
function baseOf(e: Entry, knobs: Knob[], k: number): Record<string, number> {
  const out: Record<string, number> = { ...e.to };
  const byId = new Map(knobs.map((x) => [x.id, x]));
  for (const id of Object.keys(e.to)) {
    const knob = byId.get(id);
    if (isStep(knob)) out[id] = e.steps[id];
    else if (!knob?.device && k < 1 && typeof e.from[id] === 'number') out[id] = e.from[id] + (e.to[id] - e.from[id]) * k;
  }
  return out;
}

const progress = (m: Morph) => (m.length > 0 ? ease(clamp(m.elapsed / m.length, 0, 1)) : 1);

/** A step to take at its module's next neutral (its dip begun, or turned down again). */
function dipFor(m: Morph, module: string, pending: Record<string, number>) {
  const d = (m.dips[module] ??= { depth: 0, down: true, pending: {} });
  d.down = true;
  Object.assign(d.pending, pending);
}

/** Morphs to `modules` (a state of a chain, or another chain) over `seconds`, from where everything is shown now. */
export function morphTo(m: Morph, modules: Module[], seconds: number, types: Record<string, ModuleType>): void {
  const k = progress(m);
  const now = new Map(m.entries.map((e) => [`${e.id}|${e.type}`, e]));
  const kept = new Set<string>();
  const entries: Entry[] = [];
  for (const mod of modules) {
    const key = `${mod.id}|${mod.type}`;
    const knobs = knobsOf(types, mod.type);
    const was = now.get(key);
    kept.add(key);
    if (was) {
      const steps = { ...was.steps };
      const changed: Record<string, number> = {};
      for (const [id, v] of Object.entries(stepsOf(knobs, mod.knobs))) if (steps[id] !== v) changed[id] = v;
      // A pending step the new state no longer asks for is dropped.
      const dip = m.dips[mod.id];
      if (dip) for (const id of Object.keys(dip.pending)) if (!(id in changed)) delete dip.pending[id];
      if (Object.keys(changed).length) dipFor(m, mod.id, changed);
      else if (dip) dip.down = false;
      entries.push({ id: mod.id, type: mod.type, from: baseOf(was, knobs, k), to: { ...mod.knobs }, steps, leaving: false });
    } else {
      delete m.dips[mod.id];
      entries.push({ id: mod.id, type: mod.type, from: neutralOf(knobs, mod.knobs), to: { ...mod.knobs }, steps: stepsOf(knobs, mod.knobs), leaving: false });
    }
  }
  const ids = new Set(modules.map((x) => x.id));
  for (const e of m.entries) {
    if (kept.has(`${e.id}|${e.type}`)) continue;
    const knobs = knobsOf(types, e.type);
    const base = baseOf(e, knobs, k);
    entries.push({ id: e.id, type: e.type, from: base, to: neutralOf(knobs, base), steps: { ...e.steps }, leaving: true });
  }
  for (const id of Object.keys(m.dips)) if (!ids.has(id)) delete m.dips[id];
  m.entries = entries;
  m.elapsed = 0;
  m.length = Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
}

/** A moment (evaluate.ts's `moments`): `knob`, a step of `module`, to `next` at its next neutral; by default its next
 *  whole step, round to its least after its most. */
export function askStep(m: Morph, module: string, knob: string, types: Record<string, ModuleType>, next?: number): void {
  const e = m.entries.find((x) => x.id === module && !x.leaving);
  const k = e && knobsOf(types, e.type).find((x) => x.id === knob);
  if (!e || !k || !isStep(k)) return;
  const pending = m.dips[module]?.pending[knob];
  const at = pending ?? e.steps[knob];
  const value = next ?? (Math.floor(at) + 1 > k.max ? k.min : Math.floor(at) + 1);
  e.to[knob] = clamp(value, k.min, k.max);
  dipFor(m, module, { [knob]: e.to[knob] });
}

/** One frame, `dt` seconds on; `beat` the seconds a beat lasts now (null: not known). */
export function stepMorph(m: Morph, dt: number, beat: number | null, types: Record<string, ModuleType>): MorphFrame {
  const step = Number.isFinite(dt) ? Math.max(0, dt) : 0;
  const half = (beat !== null && Number.isFinite(beat) && beat > 0 ? beat : FALLBACK_BEAT) / 2;
  m.elapsed += step;
  const k = progress(m);
  const knobs: MorphFrame['knobs'] = {};
  const leaving: Leaving[] = [];
  for (const e of m.entries) {
    const ks = knobsOf(types, e.type);
    const out = baseOf(e, ks, k);
    const dip = e.leaving ? undefined : m.dips[e.id];
    if (dip) {
      let shown: number;
      if (dip.down) {
        dip.depth += step / half;
        shown = dip.depth;
        if (dip.depth >= 1 - EPS) {
          // The bottom: shown at neutral this frame, the step taken; the time past it already on the way back, so the
          // frames after are the same at any rate.
          shown = 1;
          dip.depth = Math.max(0, 2 - dip.depth);
          dip.down = false;
          Object.assign(e.steps, dip.pending);
          for (const [id, v] of Object.entries(dip.pending)) out[id] = v;
          dip.pending = {};
        }
      } else shown = dip.depth -= step / half;
      if (!dip.down && shown <= EPS) delete m.dips[e.id];
      else {
        const levels = ks.filter((x) => x.kind === 'level' && !x.device);
        const dipped = levels.length ? levels : ks.filter((x) => x.kind !== 'step' && !x.device);
        const d = ease(clamp(shown, 0, 1));
        for (const x of dipped) if (typeof out[x.id] === 'number') out[x.id] += (x.neutral - out[x.id]) * d;
      }
    }
    if (e.leaving) leaving.push({ id: e.id, type: e.type, knobs: out });
    else knobs[e.id] = out;
  }
  const done = m.elapsed >= m.length - EPS;
  if (done && (m.length > 0 || m.entries.some((e) => e.leaving))) {
    m.entries = m.entries.filter((e) => !e.leaving);
    for (const e of m.entries) e.from = { ...e.to };
    m.length = 0;
  }
  return { knobs, leaving, still: done && !Object.keys(m.dips).length };
}
