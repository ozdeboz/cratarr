// A recipe's precheck (decision 0046, amended): before the dice show a complete randomisation, it is run against the
// playing track's own signals for half a minute (from where it plays), through the visuals' own mapping (aspects.ts,
// the music as musicOf gives it), and measured (meters.ts); one that would go over the governor's limits is refused,
// and the dice draw another. No picture is drawn: the flashes are judged from the brightness the whole picture would
// pulse with (the glow, and the parts' lights), one tile for the screen, as the runtime's limit judges a quarter of it.
//
// Done in slices (`prechecking`), so a slow tablet keeps its frames: a step is a few hundredths of a millisecond.

import { newAspects, stepAspects } from './aspects';
import type { Beats } from './beats';
import { BUDGETED, budgetOf } from './budget';
import { settingsReport } from './effects';
import { Meters, screenTile, type MeterValues } from './meters';
import { musicOf, newMusicState } from './music.svelte';
import { settingsOf, type Recipe, type Scope } from './recipe';
import { FLUID_FULL, type VisualSettings } from './settings.svelte';
import type { Signals } from './signals';

/** The limits a recipe is held to (the governor's, at the Calm ↔ Wild setting). */
export type Limits = Partial<Record<'flash' | 'motion' | 'shapeChurn' | 'colourChurn' | 'zoomPumping' | 'looksPerMinute', number>>;

/** Until the owner's ratings fit them (fit.ts), on the governor's scale (budget.ts): a little above (a quarter more, up
 *  to the next 0.05) the most Standard did over 11 varied tracks, at a quarter and three fifths of each, three track
 *  looks a track as tracks draw them (65, 2026-10-18, 506 runs; 26 of the 66 looks with effects; Standard's typical, its
 *  runs' median mean, and its most: motion 0.27 and 0.47, shape 0.28 and 0.68 (64: 0.16 and 0.42, before the meters
 *  saw Presence move the points between the parts), colour 0.04 and 0.35, pumping 0.07 and 0.35; flashes 1 and 3), so
 *  today's behaviour always passes, quiet passage or loud, and the wildest tenth or so of recipes does not (34 of 440).
 *  Flashes: three a second over a quarter of the screen, the rule. */
export const DEFAULT_LIMITS: Required<Limits> = {
  flash: 3,
  motion: 0.6,
  shapeChurn: 0.9,
  colourChurn: 0.45,
  zoomPumping: 0.45,
  // New looks a minute (the dice's and the moments', a track's own not counted): Standard has none of its own; four
  // keeps a moment-driven recipe from turning into a slideshow.
  looksPerMinute: 4,
};

export type Checked = {
  ok: boolean;
  /** Each meter's most and mean over the run (after its first seconds, while the meters settle): the budgeted ones on
   *  the governor's scale (budget.ts), the flashes as counted. */
  worst: Partial<MeterValues>;
  mean: Partial<MeterValues>;
  /** The meters over their limits, by name. */
  over: string[];
};

/** How long a recipe is run (s), at how many frames a second, and how long the meters settle first. */
export const SECONDS = 30;
const FPS = 30;
const SETTLE_S = 3;
/** How bright the flame's dense parts are on the screen, about (relative luminance): the flashes' scale. */
const BRIGHT = 0.35;
/** Steps a slice: about 1 ms on a PC (a run is 25 ms there, 2026-10-18), a few on a slow tablet. */
const SLICE = 30;

/** The run, a slice at a time; its value the verdict. */
export function* prechecking(
  recipe: Recipe,
  base: VisualSettings,
  signals: Signals,
  beats: Beats | null,
  from: number,
  limits: Limits,
  seconds = SECONDS,
  scope: Scope = 'all',
): Generator<void, Checked> {
  const settings = settingsOf(recipe, base, scope);
  const aspects = newAspects();
  const state = newMusicState();
  const meters = new Meters();
  const worst: Record<string, number> = {};
  const sum: Record<string, number> = {};
  let counted = 0;
  const dt = 1 / FPS;
  const frames = Math.round(seconds * FPS);
  for (let k = 0; k < frames; k++) {
    if (k > 0 && k % SLICE === 0) yield;
    const t = from + k * dt;
    const m = musicOf(signals, beats, t, true, { state, at: t * 1000, lighting: settings.lighting });
    const a = stepAspects(aspects, m, settings, dt, { still: false, now: t * 1000, fluidFull: FLUID_FULL });
    const lights = a.hits.reduce((s, h) => s + Math.max(0, h), 0) / Math.max(1, a.hits.length);
    const luminance = Math.min(1, BRIGHT * a.flash * (1 + 0.6 * lights));
    meters.add({
      t: k * dt,
      turnRates: a.rates,
      shape: a.shape,
      weights: a.parts.map((p) => p.weight ?? 1),
      hue: a.parts.map((p) => p.hue),
      spread: a.parts.map((p) => p.spread ?? 1),
      scale: a.scale,
      saturation: a.saturation,
      look: null,
      symmetry: a.moments.symmetry,
      tiles: [screenTile(k * dt, luminance)],
      // What the look's effects add, as the visuals' meters take it (65): so a recipe's effects are held to the limits.
      extra: settingsReport(settings, a.effects),
    });
    if (k * dt < SETTLE_S) continue;
    // The budgeted meters over their budget at the music's activity now (Wild at its middle), as the governor holds
    // them; so a limit means the same here as there, loud passage or quiet.
    const budget = budgetOf(1, 0.5, a.lit);
    const now = meters.now();
    counted++;
    for (const [key, v] of Object.entries(now)) {
      if (typeof v !== 'number') continue;
      const x = (BUDGETED as readonly string[]).includes(key) ? v / budget : v;
      worst[key] = Math.max(worst[key] ?? 0, x);
      sum[key] = (sum[key] ?? 0) + x;
    }
  }
  const over = Object.entries(limits)
    .filter(([key, limit]) => limit !== undefined && (worst[key] ?? 0) > limit)
    .map(([key]) => key);
  const mean = Object.fromEntries(Object.entries(sum).map(([key, v]) => [key, v / Math.max(1, counted)]));
  return { ok: over.length === 0, worst: worst as Partial<MeterValues>, mean: mean as Partial<MeterValues>, over };
}

/** The run all at once (for tests and the calibration). */
export function precheck(recipe: Recipe, base: VisualSettings, signals: Signals, beats: Beats | null, from: number, limits: Limits, seconds = SECONDS, scope: Scope = 'all'): Checked {
  const run = prechecking(recipe, base, signals, beats, from, limits, seconds, scope);
  for (;;) {
    const r = run.next();
    if (r.done) return r.value;
  }
}
