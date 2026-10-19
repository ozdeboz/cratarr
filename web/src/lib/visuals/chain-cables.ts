// Cables from the patch's extras into today's chain (decision 0056): a modulator cabled into the flame's Hue, a signal
// with its own reference and speed into the show's Glow. Today's chain is aspects.ts's: its knobs are the routes'
// columns (patch.ts `ASPECT_KNOB`). So such a cable acts as a route into its column does, added to what the routes give
// (`routes.ts` `give`) before aspects.ts takes it: a swing adds what its curve gives at its x (times the envelope, but
// the loudness's own, and its walk), a level lets through `letThrough` of its setting, the cables multiplying. A band's
// cable into a part's column (Shape, Hue, Spread, Rotation, Lights, Presence) is that band's part's own; into another,
// the bands' mean (a level's: their geometric mean; a moment's: their largest), as the routes' bands are.
//
// The x a cable is read at is the evaluator's (evaluate.ts): a signal's clamp(s / 2, 0, 1), 1 = typical half way; a
// modulator's output as it is (a bipolar one's (v + 1) / 2), as the evaluator last stepped it. A knob that is no
// column (the look's Detail, a band's Linger …) is moved as the evaluator moves a knob (`moveKnob`), into the setting
// that frame takes in its place (`cabledSettings`): the look's as the flame is made, Purity as it is drawn, the bands'
// times and the frame's behaviour as the aspects take them. The device's (Quality, FPS, Brightness) are not cabled.

import { heightOf, letThrough, moveKnob, shapeOf } from './evaluate';
import { MODULATORS, type ModulatorType } from './modulators/modulators';
import { ASPECT_KNOB, SETTING_KNOB, type Cable, type Module, type ModuleType, type SignalRef } from './patch';
import { A, ASPECTS, LEVEL_ASPECTS, MOMENT_ASPECTS, PART_ASPECTS, type Aspect, type Given } from './routes';

const IS_LEVEL = ASPECTS.map((a) => LEVEL_ASPECTS.includes(a));
const IS_PART = ASPECTS.map((a) => PART_ASPECTS.includes(a));
const IS_MOMENT = ASPECTS.map((a) => (MOMENT_ASPECTS as readonly Aspect[]).includes(a));
/** The fluid's columns: read by stirring.ts at its own moments, through its gain (`cableGain`), not given here. */
const STIRRING = new Set<number>([A.stir, A.flow, A.swirl]);
/** The walks' columns: the Track row into them is a track's event, not a walk (aspects.ts `walkOf`): a Track cable
 *  into them is not given. */
const WALKS = new Set<number>([A.look, A.behaviour]);
/** Halved with reduced motion, as aspects.ts halves them for the routes. */
const HALVED_STILL = new Set(['beat', 'downbeat', 'section']);

/** The signals as the cables read them: with reduced motion (`still`), the beats' and sections' halved. */
export const cableSignals = (signals: (ref: SignalRef) => number, still: boolean) =>
  still ? (ref: SignalRef) => (HALVED_STILL.has(ref.signal) && ref.band === undefined ? 0.5 : 1) * signals(ref) : signals;
/** Each chain knob's column (`module|knob` → its place in `ASPECTS`). */
const COLUMN = new Map<string, number>((Object.entries(ASPECT_KNOB) as [Aspect, [string, string]][]).map(([a, [m, k]]) => [`${m}|${k}`, A[a]]));

/** The column a cable reaches, or null where it reaches none of today's. */
export const columnOf = (c: Cable): number | null => COLUMN.get(`${c.to.module}|${c.to.knob}`) ?? null;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const finite = (v: unknown, or = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : or);

/** How a cable's x is read: the frame's signals, and each modulator's last output (its id → its value, as the
 *  evaluator keeps them: `EvalState.values`), with the modulators the patch has (their types, to tell a bipolar one). */
export type CableSources = {
  signals: (ref: SignalRef) => number;
  outputs: Record<string, number>;
  modulators: readonly Module[];
  types?: Record<string, ModuleType>;
};

/** A cable's x (0..1), or null where its modulator is not there. */
export function cableX(c: Cable, src: CableSources): number | null {
  if ('modulator' in c.from) {
    const id = c.from.modulator;
    const m = src.modulators.find((x) => x.id === id);
    if (!m || !(id in src.outputs)) return null;
    const t = (src.types?.[m.type] as Partial<ModulatorType> | undefined) ?? MODULATORS[m.type];
    const v = finite(src.outputs[id]);
    return clamp(t?.range === 'bipolar' ? (v + 1) / 2 : v, 0, 1);
  }
  return clamp(finite(src.signals(c.from)) / 2, 0, 1);
}

/** Adds what `cables` into today's columns give to `out` (as `give` left it): `envelope` the routes' (0..1), `walk`
 *  each walking cable's factor by its id (1: as set), `bands` the bands the routes have. Cables into no column are
 *  left. */
export function giveCables(out: Given, cables: readonly Cable[], src: CableSources, envelope: number, walk: Record<string, number> = {}, bands = out.band.length) {
  const n = Math.max(0, Math.min(bands, out.band.length));
  let bandSum: Map<number, number> | null = null;
  // A moment's band cables join the routes' bands (`give` left them in out.band), so the largest is taken over both.
  let bandMost: Map<number, number> | null = null;
  let bandLevel: Map<number, Float64Array> | null = null;
  const e0 = clamp(finite(envelope, 1), 0, 1);
  for (const c of cables) {
    const a = columnOf(c);
    if (a === null || STIRRING.has(a) || (WALKS.has(a) && 'signal' in c.from && c.from.signal === 'track')) continue;
    const x = cableX(c, src);
    if (x === null) continue;
    const band = 'signal' in c.from && c.from.band !== undefined ? c.from.band : null;
    if (band !== null && !(Number.isInteger(band) && band >= 0 && band < n)) continue;
    const s = shapeOf(c.value);
    const y = heightOf(s, x);
    const w = s.walks ? finite(walk[c.id], 1) : 1;
    const e = 'signal' in c.from && c.from.signal === 'loudness' && band === null ? 1 : e0;
    if (IS_LEVEL[a]) {
      const through = letThrough(s.peak, y, e, w);
      if (band === null) out.level[a] *= through;
      else {
        bandLevel ??= new Map();
        const per = bandLevel.get(a) ?? bandLevel.set(a, new Float64Array(n).fill(1)).get(a)!;
        per[band] *= through;
      }
      continue;
    }
    const v = e * y * w;
    if (band === null) out.whole[a] += v;
    else if (IS_PART[a]) out.band[band][a] += v;
    else if (IS_MOMENT[a]) {
      bandMost ??= new Map();
      if (!bandMost.has(a)) bandMost.set(a, Math.max(...out.band.slice(0, n).map((b) => b[a])));
      out.band[band][a] += v;
    } else {
      bandSum ??= new Map();
      bandSum.set(a, (bandSum.get(a) ?? 0) + v);
    }
  }
  if (!n) return;
  for (const [a, sum] of bandSum ?? []) out.whole[a] += sum / n;
  for (const [a, was] of bandMost ?? []) out.whole[a] += Math.max(...out.band.slice(0, n).map((b) => b[a])) - was;
  for (const [a, per] of bandLevel ?? []) out.level[a] *= per.reduce((p, v) => p * v, 1) ** (1 / n);
}

/** Today's settings a cable may move that are no column: the look's that make the flame (moved by degrees: each a step
 *  of `LOOK_STEPS` of its range, so a cable moving one is not a flame made again every frame), Purity, and the frame's
 *  behaviour the aspects take; and the bands' times, each band's own knob (`lighting.2`). */
export const LOOK_SETTINGS = ['detail', 'twist', 'warp', 'lightness', 'vividness'] as const;
export const CABLED_SETTINGS = [...LOOK_SETTINGS, 'purity', 'intensity', 'flash', 'speed'] as const;
export const BAND_SETTINGS = ['lighting', 'evolution', 'continuity'] as const;
const LOOK_STEPS = 32;

/** What the cables make of those settings this frame (only those a cable reaches). */
export type Cabled = { settings: Partial<Record<(typeof CABLED_SETTINGS)[number], number>>; bands: Partial<Record<(typeof BAND_SETTINGS)[number], number[]>> };

/** Those settings as their cables move them from `s` (the settings as set), by the evaluator's rule; `types` the chain's
 *  module types (patch.ts `cratarrTypes`, each knob's range). */
export function cabledSettings(
  s: Record<string, unknown>,
  cables: readonly Cable[],
  src: CableSources,
  envelope: number,
  types: Record<string, ModuleType>,
  walk: Record<string, number> = {},
  bands = 6,
): Cabled {
  const out: Cabled = { settings: {}, bands: {} };
  if (!cables.length) return out;
  const into = new Map<string, Cable[]>();
  for (const c of cables) {
    const key = `${c.to.module}|${c.to.knob}`;
    (into.get(key) ?? into.set(key, []).get(key)!).push(c);
  }
  const x = (c: Cable) => cableX(c, src);
  const w = (c: Cable) => finite(walk[c.id], 1);
  const e = clamp(finite(envelope, 1), 0, 1);
  const knobOf = (module: string, id: string) => types[module]?.knobs.find((k) => k.id === id);
  for (const key of CABLED_SETTINGS) {
    const [module, id] = SETTING_KNOB[key];
    const cs = into.get(`${module}|${id}`);
    const k = knobOf(module, id);
    if (!cs || !k || k.device || k.kind === 'step') continue;
    let v = moveKnob(k, finite(s[key], k.default), cs, x, w, e, bands);
    if ((LOOK_SETTINGS as readonly string[]).includes(key)) {
      const q = (k.max - k.min) / LOOK_STEPS;
      v = q > 0 ? k.min + Math.round((v - k.min) / q) * q : v;
    }
    out.settings[key] = v;
  }
  for (const key of BAND_SETTINGS) {
    const set = Array.isArray(s[key]) ? (s[key] as number[]) : [];
    let moved: number[] | null = null;
    for (let b = 0; b < set.length; b++) {
      const cs = into.get(`flame|${key}.${b}`);
      const k = knobOf('flame', `${key}.${b}`);
      if (!cs || !k) continue;
      moved ??= set.slice();
      moved[b] = moveKnob(k, finite(set[b], k.default), cs, x, w, e, bands);
    }
    if (moved) out.bands[key] = moved;
  }
  return out;
}

/** What the cables from `signal` (a band's, or the whole track's: `band` null) into the fluid's `aspect` give at the
 *  push's own strength `x` (stirring.ts's Gain, beside its routes'): each one's curve's height there, a walking one's
 *  walk of it. Only signals: a modulator has no push of its own. */
export function cableGain(cables: readonly Cable[], signal: string, band: number | null, aspect: 'stir' | 'flow' | 'swirl', x: number, walk: Record<string, number> = {}): number {
  const [module, knob] = ASPECT_KNOB[aspect];
  let sum = 0;
  for (const c of cables) {
    if (c.to.module !== module || c.to.knob !== knob || !('signal' in c.from) || c.from.signal !== signal || (c.from.band ?? null) !== band) continue;
    const sh = shapeOf(c.value);
    sum += heightOf(sh, x) * (sh.walks ? finite(walk[c.id], 1) : 1);
  }
  return sum;
}
