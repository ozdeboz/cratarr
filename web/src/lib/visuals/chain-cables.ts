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
// column (the look's Detail, a band's Linger …) is not moved here: today's chain moves it only by its setting.

import { heightOf, letThrough, shapeOf } from './evaluate';
import { MODULATORS, type ModulatorType } from './modulators/modulators';
import { ASPECT_KNOB, type Cable, type Module, type ModuleType, type SignalRef } from './patch';
import { A, ASPECTS, LEVEL_ASPECTS, MOMENT_ASPECTS, PART_ASPECTS, type Aspect, type Given } from './routes';

const IS_LEVEL = ASPECTS.map((a) => LEVEL_ASPECTS.includes(a));
const IS_PART = ASPECTS.map((a) => PART_ASPECTS.includes(a));
const IS_MOMENT = ASPECTS.map((a) => (MOMENT_ASPECTS as readonly Aspect[]).includes(a));
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
  let bandMost: Map<number, Float64Array> | null = null;
  let bandLevel: Map<number, Float64Array> | null = null;
  const e0 = clamp(finite(envelope, 1), 0, 1);
  for (const c of cables) {
    const a = columnOf(c);
    if (a === null) continue;
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
      (bandMost.get(a) ?? bandMost.set(a, new Float64Array(n)).get(a)!)[band] += v;
    } else {
      bandSum ??= new Map();
      bandSum.set(a, (bandSum.get(a) ?? 0) + v);
    }
  }
  if (!n) return;
  for (const [a, sum] of bandSum ?? []) out.whole[a] += sum / n;
  for (const [a, per] of bandMost ?? []) out.whole[a] += Math.max(...per);
  for (const [a, per] of bandLevel ?? []) out.level[a] *= per.reduce((p, v) => p * v, 1) ** (1 / n);
}
