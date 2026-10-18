// The governor (decision 0046, amended): the last of the behaviour's three safety layers (a recipe's caps, its precheck,
// then this, as it plays). Each kind of busyness has its meter (meters.ts) and a lever that holds it back while it goes
// over its budget, and lets go again once it is back under:
// - flashes over a quarter of the screen (WCAG's three a second, over the area the owner chose: a fixed safety rule,
//   always on) hold back the Flash setting's swing;
// - motion, the turning's pace (and the fluid's);
// - shape churn, the parts' evolving (their Evolution times longer) and Drift's walk;
// - colour churn, the hues' swing;
// - zoom pumping, the size's swing;
// - new looks a minute, the moments (further apart).
// The other budgets are the standing limits (fitted to the owner's ratings, fit.ts), scaled by the Calm ↔ Wild setting
// and by how busy the music is (a quiet passage gets less). While the rating page runs a trial they are off; the
// flashes' rule stays on. Pure: the loop steps it with the meters now.

import { BUDGETED, budgetOf, type Budgeted } from './budget';
import type { MeterValues } from './meters';
import { DEFAULT_LIMITS } from './precheck';

export { BUDGETED, budgetOf, type Budgeted };

/** What the governor holds back: each lever's share of what the settings ask (1: none held back). `evolution` and `gap`
 *  stretch (the evolving's times, the moments' gap: 1 and up); the rest scale down (1 and under). */
export type Levers = { swing: number; pace: number; evolution: number; hue: number; size: number; gap: number };
export const FREE: Readonly<Levers> = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };

/** Each budgeted meter's lever, and how far that lever may go. */
export const LEVERS: Readonly<Record<Budgeted, { lever: keyof Levers; most: number }>> = {
  motion: { lever: 'pace', most: 0.25 },
  shapeChurn: { lever: 'evolution', most: 4 },
  colourChurn: { lever: 'hue', most: 0.2 },
  zoomPumping: { lever: 'size', most: 0.2 },
  looksPerMinute: { lever: 'gap', most: 6 },
};

/** The standing limits: the precheck's, a little above the most today's defaults did over eleven varied tracks
 *  (precheck.ts), until the owner's ratings fit them (fit.ts). */
export const STANDING: Readonly<Record<Budgeted, number>> = Object.fromEntries(BUDGETED.map((k) => [k, DEFAULT_LIMITS[k]])) as Record<Budgeted, number>;

/** The flashes' rule: held back above this many a second (WCAG's three, with room to act before it), let go again at
 *  it or under (the meter reads in halves; a lower mark held a beat a second at 120 BPM, 2, back for good). */
const FLASH_HOLD = 2.5;
/** After flashes over the rule, the swing stays held this long (s) before it opens again: the meter tells of a burst a
 *  second late, and a passage that flashed comes back (a lull between two left the next one's start unheld). */
const FLASH_REST = 8;
/** How fast a lever closes while its meter is over (a share a second: a half in about a second), and opens again once
 *  under four fifths of its budget (all of the way in about ten seconds). */
const CLOSE = 0.7;
const OPEN = 0.1;

/** The levers, the governor's own clock (s), and when the flashes last went over the rule on it. */
export type Governor = { levers: Levers; t: number; flashedAt: number };

export function newGovernor(): Governor {
  return { levers: { ...FREE }, t: 0, flashedAt: -Infinity };
}


/** One step, `dt` seconds on, with the meters now: the levers as they hold back. `budgets` false (a trial): only the
 *  flashes' rule. */
export function stepGovernor(
  g: Governor,
  m: MeterValues,
  opts: { dt: number; limits: Readonly<Record<Budgeted, number>>; wild: number; activity: number; budgets: boolean },
): Levers {
  const l = g.levers;
  const { dt } = opts;
  const flashing = Math.max(m.flash, m.redFlash);
  g.t += dt;
  if (flashing > FLASH_HOLD) {
    g.flashedAt = g.t;
    l.swing = Math.max(0.1, l.swing * Math.exp(-2 * CLOSE * dt));
  } else if (g.t - g.flashedAt > FLASH_REST) l.swing = Math.min(1, l.swing + OPEN * dt);
  for (const k of BUDGETED) {
    const { lever, most } = LEVERS[k];
    const limit = opts.budgets ? budgetOf(opts.limits[k], opts.wild, opts.activity) : Infinity;
    const v = m[k];
    const stretches = most > 1;
    if (v > limit) l[lever] = stretches ? Math.min(most, l[lever] * Math.exp(CLOSE * dt)) : Math.max(most, l[lever] * Math.exp(-CLOSE * dt));
    else if (v < 0.8 * limit) l[lever] = stretches ? Math.max(1, l[lever] - OPEN * (most - 1) * dt) : Math.min(1, l[lever] + OPEN * dt);
  }
  return l;
}

/** Whether a lever holds anything back (enough to show). */
export const holding = (l: Levers, lever: keyof Levers) => (lever === 'evolution' || lever === 'gap' ? l[lever] > 1.05 : l[lever] < 0.95);
