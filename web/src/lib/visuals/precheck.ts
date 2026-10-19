// A recipe's precheck (decision 0046, amended): before the dice show a complete randomisation, it is run against the
// playing track's own signals for half a minute (from where it plays), through the visuals' own mapping (aspects.ts,
// the music as musicOf gives it), and measured (meters.ts); one that would go over the governor's limits is refused,
// and the dice draw another. No picture is drawn: the flashes are judged from the brightness the whole picture would
// pulse with (the glow, and the parts' lights), one tile for the screen, as the runtime's limit judges a quarter of it.
//
// Done in slices (`prechecking`), so a slow tablet keeps its frames: a step is a few hundredths of a millisecond.

import { newAspects, stepAspects, wavesMotion, zoomsMotion, type AspectsNow } from './aspects';
import type { Beats } from './beats';
import { BUDGETED, budgetOf } from './budget';
import { settingsReport, type Report } from './effects';
import { flameOf, PRESET_KEYS } from './flame';
import { Meters, screenTile, type MeterValues } from './meters';
import { musicOf, newMusicState } from './music-core';
import { settingsOf, type Recipe, type Scope } from './recipe';
import { A } from './routes';
import { FLUID_FULL, type VisualSettings } from './settings.svelte';
import type { Signals } from './signals';
import { DIVE_MOST, diveTurnOf } from './zoom';

// The limits (budget.ts), here too for those that took them from the precheck.
export { DEFAULT_LIMITS, type Limits } from './budget';
import { DEFAULT_LIMITS, type Limits } from './budget';

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

/** The dive's turn a second at a dive rate of 1 for a look (zoom.ts `diveTurnOf`: the turn is in proportion to the
 *  rate), kept by the look: fitting the part to dive into takes a few milliseconds on a PC, several times that on a
 *  tablet, and a roll's eight recipes share their base's look. */
const diveTurns = new Map<string, number>();
function diveTurnPerRate(settings: VisualSettings): number {
  const look = PRESET_KEYS.map((k) => settings[k]).join('|');
  let turn = diveTurns.get(look);
  if (turn === undefined) {
    if (diveTurns.size > 64) diveTurns.clear();
    turn = diveTurnOf(flameOf(settings), 1);
    diveTurns.set(look, turn);
  }
  return turn;
}

/** A report with `motion` more (a second). */
const withMotion = (r: Report, motion: number): Report => (motion > 0 ? { ...r, motion: (r.motion ?? 0) + motion } : r);

/** A frame's mapping, measured as the visuals measure it, without a picture: the flashes from the brightness the whole
 *  picture would pulse with (the glow, the parts' lights), one tile for the screen. The precheck's, and the one
 *  engine's sidechaining's tests (decision 0057). `divePerRate`: the dive's turn a second at a rate of 1 (0: none). */
export function measure(meters: Meters, a: AspectsNow, t: number, settings: VisualSettings, divePerRate = 0) {
  const lights = a.hits.reduce((s, h) => s + Math.max(0, h), 0) / Math.max(1, a.hits.length);
  const luminance = Math.min(1, BRIGHT * a.flash * (1 + 0.6 * lights));
  meters.add({
    t,
    turnRates: divePerRate && a.dive ? a.rates.map((r) => r + DIVE_MOST * a.dive * divePerRate) : a.rates,
    shape: a.shape,
    weights: a.parts.map((p) => p.weight ?? 1),
    hue: a.parts.map((p) => p.hue),
    spread: a.parts.map((p) => p.spread ?? 1),
    scale: a.scale,
    saturation: a.saturation,
    // A moment (another symmetry, lens, effect…) is a new look, as the visuals count it (65.1: it was never counted).
    look: Object.values(a.moments).some(Boolean) ? 'moment' : null,
    symmetry: a.moments.symmetry,
    tiles: [screenTile(t, luminance)],
    // What the look's effects add, the waves where nothing flows and the zooms' streaming out, as the visuals' meters
    // take them (65): so a recipe's effects, waves and zooms are held to the limits.
    extra: withMotion(settingsReport(settings, a.effects, undefined, false, a.routed), wavesMotion(a.given[A.waves], a.fluid) + zoomsMotion(a.tunnel, a.dive)),
  });
}

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
  // The dive's turn, as the visuals count it (the renderer's, beside each part's own); none where it cannot dive.
  const divePerRate = settings.dive > 0 ? diveTurnPerRate(settings) : 0;
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
    measure(meters, a, k * dt, settings, divePerRate);
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
