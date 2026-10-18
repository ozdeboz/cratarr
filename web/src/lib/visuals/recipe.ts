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
import type { BEHAVIOUR_PINS, VisualSettings } from './settings.svelte';

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
  { signal: 'section', aspect: 'look', lo: 0.3, hi: 1, family: 'look' },
  { signal: 'downbeat', aspect: 'look', lo: 0.1, hi: 0.4, family: 'look' },
  { signal: 'section', aspect: 'behaviour', lo: 0.2, hi: 0.6, family: 'behaviour' },
  { signal: 'swell', aspect: 'behaviour', lo: 0.1, hi: 0.4, family: 'behaviour' },
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
    // Fluid's slider is the most a route gives it (64.1): where a route goes to it, some to give; else half the time
    // none.
    fluid: Object.keys(r).some((k) => k.endsWith('|fluid')) || rnd() < 0.5 ? between(0.2, 0.7) : 0,
    smoothing: between(0.3, 0.7),
    floor: between(0, 0.25),
    depth: between(0.35, 0.65),
    lighting: LIGHTING.map((v) => v * 2 ** between(-0.7, 1)),
    evolution: EVOLUTION.map((v) => v * 2 ** between(-0.7, 1.3)),
    continuity: Array.from({ length: bands }, () => between(0.2, 0.9)),
  };
  return { seed, routes: r, settings, extras };
}

/** What the dice's double tap randomises (the owner: "double tap becomes this randomize 1 to 5"): how the visuals move
 *  with the music, the routes into the flame's form, its lights, its turning and its size, the turning's pace and the
 *  swing, and the bands' times; the look, its colours, the grain and smoke, Intensity and every other route stay. */
/** The columns of the fluid's stirring, which a roll leaves as they are. */
export const STIRRING = ['stir', 'flow', 'swirl'] as const;

export const MOVEMENT = {
  columns: ['shape', 'lights', 'rotation', 'size'],
  settings: ['flash', 'speed', 'lighting', 'evolution', 'continuity'],
} as const;

/** The settings a recipe makes of `base`: its routes whole (every other route off) and its settings; `base`'s look,
 *  pins, colours and what the device can afford (Points, Steps, Resolution, FPS, Brightness) kept, and of the rest
 *  what the owner pinned: a behaviour slider, a route ('route:<key>', on or off as it is). The Track row (what a new track does to the look and the behaviour) is base's: a
 *  recipe is how the visuals move with the music, not what a new track brings. */
/** What a roll changes: everything, or how it moves (`MOVEMENT`). */
export type Scope = 'all' | 'movement';

export function settingsOf(recipe: Recipe, base: VisualSettings, scope: Scope = 'all'): VisualSettings {
  const within = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
  const r = recipe.settings;
  const pinned = new Set(base.pins);
  // Of a movement roll, only the movement's columns are the recipe's.
  const moving = (key: string) => MOVEMENT.columns.some((c) => key.endsWith(`|${c}`));
  // The Track row and the fluid's stirring (Stir, Flow, Swirl: how the music pushes the fluid, its defaults today's
  // stirring) are base's: a roll that dropped them would leave the fluid unstirred.
  const stirring = (key: string) => STIRRING.some((c) => key.endsWith(`|${c}`));
  const kept = (key: string) =>
    key.startsWith('track|') ||
    stirring(key) ||
    pinned.has(`route:${key}`) ||
    (scope === 'movement' && !moving(key));
  const routes: VisualSettings['routes'] = {};
  for (const [k, v] of Object.entries(recipe.routes)) if (!kept(k)) routes[k] = v;
  for (const [k, v] of Object.entries(base.routes)) if (kept(k)) routes[k] = v;
  // A behaviour slider pinned, or not the movement's in a movement roll, is base's.
  const takes = (k: string) => scope === 'all' || (MOVEMENT.settings as readonly string[]).includes(k);
  const one = <K extends (typeof BEHAVIOUR_PINS)[number]>(k: K, v: number) => (pinned.has(k) || !takes(k) ? base[k] : v);
  return {
    ...base,
    routes,
    flash: one('flash', within(r.flash, 0, 2)),
    speed: one('speed', within(r.speed, 0, 2)),
    intensity: one('intensity', within(r.intensity, 0, 1)),
    fluid: one('fluid', within(r.fluid, 0, 1)),
    smoothing: one('smoothing', within(r.smoothing, 0, 1)),
    floor: one('floor', within(r.floor, 0, 1)),
    depth: one('depth', within(r.depth, 0, 1)),
    // Each band's times as the recipe has them, but a pinned one (`lighting:2`).
    lighting: takes('lighting') ? r.lighting.map((v, b) => (pinned.has(`lighting:${b}`) ? base.lighting[b] : within(v, 0.02, 1))) : base.lighting,
    evolution: takes('evolution') ? r.evolution.map((v, b) => (pinned.has(`evolution:${b}`) ? base.evolution[b] : within(v, 0.02, 2))) : base.evolution,
    continuity: takes('continuity') ? r.continuity.map((v, b) => (pinned.has(`continuity:${b}`) ? base.continuity[b] : within(v, 0, 1))) : base.continuity,
  };
}
