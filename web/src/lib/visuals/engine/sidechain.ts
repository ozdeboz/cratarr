// Standard sidechaining (decision 0057; the owner: "it's just standard sidechaining"): each kind of busyness the meters measure (meters.ts) keys a ducker (compressor.ts) that holds back
// the knobs of that kind, through their cables. A ducker, not a ratio compressor: the meters count (flashes a second,
// looks a minute), and a reduction in proportion to how far over would settle with the count still over; a ducker
// holds its depth down while the key is over.
//
// - **Flash** (WCAG 2.3.1, a safety rule, not taste): keyed by the flashes a second over a quarter of the screen,
//   threshold 2.5 (three, with room to act), down to a tenth of the Accent's swing at once, held 8 s after the
//   last burst, let back over a few seconds. Fixed: always on (in a trial too), no knob weakens it, Wild included.
// - **Motion, shape churn, colour churn, zoom pumping, looks a minute**: keyed by their meters against their budgets
//   (budget.ts: the standing limits, scaled by Wild and the music's activity), down to as far as each lever went before
//   (the turning's pace to a quarter, the hues' and the size's swing to a fifth; the evolving four times slower, the
//   looks six times further apart), coming in over about a second and a half, let back once under four fifths.

import { BUDGETED, budgetOf, DEFAULT_LIMITS, type Budgeted } from '../budget';
import type { MeterValues } from '../meters';
import { newDucker, stepDucker, type DuckerKnobs, type DuckerState } from './compressor';

/** The standing limits of each kind of busyness (budget.ts's): the Duckers' thresholds start from them. */
export const STANDING: Readonly<Record<Budgeted, number>> = Object.fromEntries(BUDGETED.map((k) => [k, DEFAULT_LIMITS[k]])) as Record<Budgeted, number>;

/** The flash guard's ducker: fixed. */
export const FLASH_GUARD: Readonly<DuckerKnobs> = { threshold: 2.5, depth: 20, attack: 0, hold: 8, release: 3, hysteresis: 1 };

/** Each budgeted kind: the lever it holds, how far (its most: a share, or a stretch over 1), as before. */
export const BUSY: Readonly<Record<Budgeted, { lever: keyof Levers; most: number }>> = {
  motion: { lever: 'pace', most: 0.25 },
  shapeChurn: { lever: 'evolution', most: 4 },
  colourChurn: { lever: 'hue', most: 0.2 },
  zoomPumping: { lever: 'size', most: 0.2 },
  looksPerMinute: { lever: 'gap', most: 6 },
};
/** The busy duckers' timing: in over about 1.4 s (the levers closed at 0.7 a second), back over about 3.3 s (they
 *  opened in about ten), let back once under four fifths of the budget. */
const BUSY_TIMING = { attack: 1 / 0.7, hold: 0, release: 10 / 3, hysteresis: 0.8 };

/** What the sidechaining holds back: each lever's share of what the settings ask (1: none). `evolution` and `gap`
 *  stretch (the evolving's times, the moments' gap: 1 and up); the rest scale down (1 and under). */
export type Levers = { swing: number; pace: number; evolution: number; hue: number; size: number; gap: number };
export const FREE: Readonly<Levers> = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };

/** Whether a lever holds something back (a stretch over 1, a share under 1), as the page marks it. */
export const holding = (l: Levers, lever: keyof Levers) => (lever === 'evolution' || lever === 'gap' ? l[lever] > 1.05 : l[lever] < 0.95);

/** The levers as a patch's sidechains hold them this frame (the one engine's outputs: each kind's Ducker, the Screen's
 *  guard; 1 where a patch has none), for what the page still reads as levers. */
export const leversOf = (o: Record<string, number>): Levers => {
  // An output by its module's own id, at the top or inside a device (`calm/duck-motion.out`: the Calm device's).
  const at = (key: string) => o[key] ?? Object.entries(o).find(([k]) => k.endsWith(`/${key}`))?.[1];
  return {
    swing: at('screen.guard') ?? 1,
    pace: at('duck-motion.out') ?? 1,
    evolution: 1 / (at('duck-shape.out') ?? 1),
    hue: at('duck-colour.out') ?? 1,
    size: at('duck-zoom.out') ?? 1,
    gap: 1 / (at('duck-looks.out') ?? 1),
  };
};

export type SidechainState = { flash: DuckerState; busy: Record<Budgeted, DuckerState> };
export const newSidechain = (): SidechainState => ({ flash: newDucker(), busy: Object.fromEntries(BUDGETED.map((k) => [k, newDucker()])) as Record<Budgeted, DuckerState> });

/** One step, `dt` seconds on, with the meters now: each lever as its ducker holds it. `budgets` false (a trial): only
 *  the flash guard. */
export function stepSidechain(
  st: SidechainState,
  m: MeterValues,
  opts: { dt: number; limits: Readonly<Record<Budgeted, number>>; wild: number; activity: number; budgets: boolean },
): Levers {
  const swing = Math.max(0.1, stepDucker(st.flash, 1, Math.max(m.flash, m.redFlash), FLASH_GUARD, opts.dt).gain);
  const levers: Levers = { swing, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };
  for (const k of BUDGETED) {
    const { lever, most } = BUSY[k];
    const threshold = opts.budgets ? budgetOf(opts.limits[k], opts.wild, opts.activity) : Infinity;
    const depth = Math.abs(20 * Math.log10(most));
    const gain = stepDucker(st.busy[k], 1, m[k], { threshold, depth, ...BUSY_TIMING }, opts.dt).gain;
    levers[lever] = most > 1 ? Math.min(most, 1 / gain) : Math.max(most, gain);
  }
  return levers;
}
