// A complete randomisation's recipe (decision 0046, amended): not only another look (the stars), but another way of
// following the music. The owner wanted it beside the look-only dice, kept apart from them; the design they agreed
// with: a fixed backbone (what makes the visuals follow the music at all: loudness sets the size and colour, each band's
// level shapes and turns its part, its hits light it, the beats swell it), each of its amounts varied a little, and 3 to
// 6 more routes drawn from ones that make musical sense, each within its own cap; the settings that shape the motion
// (Flash, Speed, Intensity, the fluid, the grain, the bands' times) varied within ranges that keep it watchable. Then
// a precheck runs the recipe against the track's own signals and refuses one that would flash or churn past the
// governor's limits (`precheck`, with the frame's mapping the visuals use).
//
// Pure: the same seed gives the same recipe (every device of a sync group alike).

import { routeKey, type Aspect, type Signal } from './routes';

export type RecipeSettings = {
  flash: number;
  speed: number;
  intensity: number;
  fluid: number;
  smoothing: number;
  floor: number;
  depth: number;
  lighting: number[];
  evolution: number[];
  continuity: number[];
};

export type Recipe = {
  seed: number;
  /** Every route it sets (all others 0). */
  routes: Record<string, number>;
  settings: RecipeSettings;
  /** The extra routes it drew, by name (for the log and the rating page). */
  extras: string[];
};

/** A route that makes musical sense, its amounts' range, and its family (no two of one family in a recipe). */
type Extra = { signal: Signal; aspect: string; lo: number; hi: number; family: string };

// Track signals go to the whole picture or every part; band signals to every band alike.
const EXTRAS: Extra[] = [
  { signal: 'beat', aspect: 'brightness', lo: 0.1, hi: 0.35, family: 'glow' },
  { signal: 'downbeat', aspect: 'brightness', lo: 0.15, hi: 0.4, family: 'glow' },
  { signal: 'swell', aspect: 'brightness', lo: 0.2, hi: 0.5, family: 'glow' },
  { signal: 'downbeat', aspect: 'spread', lo: 0.1, hi: 0.4, family: 'spread' },
  { signal: 'width', aspect: 'spread', lo: 0.2, hi: 0.6, family: 'spread' },
  { signal: 'percussive', aspect: 'spread', lo: 0.1, hi: 0.4, family: 'spread' },
  { signal: 'buildUp', aspect: 'size', lo: 0.1, hi: 0.35, family: 'size' },
  { signal: 'beat', aspect: 'size', lo: 0.05, hi: 0.15, family: 'size' },
  { signal: 'buildUp', aspect: 'trails', lo: 0.2, hi: 0.6, family: 'trails' },
  { signal: 'loudness', aspect: 'trails', lo: -0.5, hi: -0.2, family: 'trails' },
  { signal: 'harmony', aspect: 'hue', lo: 0.1, hi: 0.3, family: 'hue' },
  { signal: 'key', aspect: 'hue', lo: 0.2, hi: 0.6, family: 'hue' },
  { signal: 'bandSwell', aspect: 'hue', lo: 0.05, hi: 0.2, family: 'hue' },
  { signal: 'centroid', aspect: 'saturation', lo: 0.1, hi: 0.4, family: 'colour' },
  { signal: 'harmonic', aspect: 'saturation', lo: 0.1, hi: 0.3, family: 'colour' },
  { signal: 'tempo', aspect: 'rotation', lo: 0.1, hi: 0.4, family: 'turn' },
  { signal: 'flux', aspect: 'shape', lo: 0.2, hi: 0.5, family: 'shape' },
  { signal: 'percussiveHits', aspect: 'lights', lo: 0.4, hi: 1, family: 'lights' },
  { signal: 'crest', aspect: 'lights', lo: 0.2, hi: 0.6, family: 'lights' },
  { signal: 'loudness', aspect: 'smoothing', lo: -0.4, hi: -0.1, family: 'grain' },
  { signal: 'centroid', aspect: 'depth', lo: 0.1, hi: 0.3, family: 'grain' },
  { signal: 'swell', aspect: 'fluid', lo: 0.2, hi: 0.8, family: 'fluid' },
  { signal: 'section', aspect: 'drift', lo: 0.3, hi: 1, family: 'drift' },
  { signal: 'downbeat', aspect: 'drift', lo: 0.1, hi: 0.4, family: 'drift' },
];

/** The most all of a recipe's routes into light and glow may add up to (flashes are the governor's first worry). */
const MOST_FLASHING = 1.6;
/** Today's bands' times, which a recipe varies. */
const LIGHTING = [0.2, 0.18, 0.15, 0.13, 0.1, 0.08];
const EVOLUTION = [0.25, 0.25, 0.25, 0.25, 0.25, 0.25];

const BAND_SIGNALS = new Set<Signal>(['level', 'flux', 'hits', 'percussive', 'harmonic', 'percussiveHits', 'crest', 'width', 'bandSwell']);

/** Random numbers from a seed (mulberry32). */
function randomFrom(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The recipe of `seed`, for `bands` bands and the aspects there are (`aspects`: routes.ts's ASPECTS, so a route to an
 *  aspect a version does not have is never drawn). */
export function recipeFor(seed: number, aspects: readonly string[], bands = 6): Recipe {
  const rnd = randomFrom(seed);
  const between = (lo: number, hi: number) => lo + (hi - lo) * rnd();
  const r: Record<string, number> = {};
  // Aspects by name: a route to one this version lacks (Drift before 64) is not drawn.
  const put = (signal: Signal, aspect: string, v: number) => {
    if (!aspects.includes(aspect)) return;
    const a = aspect as Aspect;
    if (BAND_SIGNALS.has(signal)) for (let b = 0; b < bands; b++) r[routeKey(signal, b, a)] = v;
    else r[routeKey(signal, null, a)] = v;
  };

  // The backbone, each amount within a range around today's.
  put('loudness', 'size', between(0.7, 1));
  put('loudness', 'saturation', between(0.6, 1));
  put('loudness', 'rotation', between(0.1, 0.4));
  put('beat', 'brightness', between(0.1, 0.3));
  put('beat', 'size', between(0.04, 0.1));
  put('level', 'shape', between(0.6, 1));
  put('level', 'rotation', between(0.3, 0.8));
  put('level', 'hue', between(0.04, 0.12));
  put('hits', 'lights', between(0.6, 1));

  // 3 to 6 extras, no two of one family, none the backbone already has.
  const extras: string[] = [];
  const families = new Set<string>();
  const want = 3 + Math.floor(rnd() * 4);
  const pool = EXTRAS.filter((e) => aspects.includes(e.aspect));
  for (let tries = 0; extras.length < want && tries < 40; tries++) {
    const e = pool[Math.floor(rnd() * pool.length)];
    const key = routeKey(e.signal, BAND_SIGNALS.has(e.signal) ? 0 : null, e.aspect as Aspect);
    if (families.has(e.family) || r[key] !== undefined) continue;
    families.add(e.family);
    put(e.signal, e.aspect, between(e.lo, e.hi));
    extras.push(`${e.signal} → ${e.aspect}`);
  }

  // The light and glow all together under their cap: scaled down as one where they would be more.
  const flashing = Object.entries(r).filter(([k]) => /\|(lights|brightness)$/.test(k));
  const sum = (signalOf: (k: string) => string) => {
    // A band signal's routes count once (they act on their own bands), a track signal's once.
    const seen = new Set<string>();
    let total = 0;
    for (const [k, v] of flashing) {
      const s = signalOf(k);
      if (seen.has(s)) continue;
      seen.add(s);
      total += Math.max(0, v);
    }
    return total;
  };
  const total = sum((k) => `${k.split('|')[0]}|${k.split('|')[2]}`);
  if (total > MOST_FLASHING) for (const [k, v] of flashing) r[k] = (v * MOST_FLASHING) / total;

  const settings: RecipeSettings = {
    flash: between(0.6, 1.4),
    speed: between(0.6, 1.5),
    intensity: between(0.6, 1),
    fluid: rnd() < 0.5 ? 0 : between(0.2, 0.7),
    smoothing: between(0.3, 0.7),
    floor: between(0, 0.25),
    depth: between(0.35, 0.65),
    lighting: LIGHTING.map((v) => v * 2 ** between(-0.7, 1)),
    evolution: EVOLUTION.map((v) => v * 2 ** between(-0.7, 1.3)),
    continuity: Array.from({ length: bands }, () => between(0.2, 0.9)),
  };
  return { seed, routes: r, settings, extras };
}
