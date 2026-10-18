// What the governor learns from the owner's thumbs (decision 0048, amended): its budgets' limits, nudged as an up-down
// staircase nudges a threshold (Levitt 1971; Kaernbach 1991's weighted up-down). The owner: "I could get a general
// thumbs up or thumbs down for any visualization that I currently look at, and it could then decide to tune", and
// "Simple is good, but we do need to handle the caveats":
//
// - A thumbs down tightens the limit of the one kind of busyness that stood out most over the 20 seconds before it
//   (how far past what the defaults typically do, as a share of how far the defaults themselves go at most: the kinds
//   range very differently, colour churn's most is ten times its typical, motion's not twice), to a step under what it
//   was then, so it acts at once. All of it in the
//   governor's own terms: each meter against its budget at a limit of 1 (as Calm ↔ Wild and the music's activity
//   scale it, governor.ts `budgetOf`), so a limit learned is one the governor holds the picture to as it is.
//   Where nothing stood out, it only counts toward the periodic fit: dislikes are pressed more often than likes, and
//   most are of the look rather than its busyness, so each one tightening something would ratchet everything calm.
// - A thumbs up loosens, a step, the limit the governor is holding the picture back by then (its amber dot): the way
//   back out once a limit is tight, without which a system learning only from what it shows narrows for good.
// - Each limit's step halves each time it turns the other way (from a tenth to a fortieth), so it settles.
// - Each drifts back toward where it started, half way in thirty days: taste moves with mood and familiarity.
// - None goes under half or over twice where it started. The flash rule is not learned.
//
// Pure: the visuals load what was learned (the server keeps it per user), drift it, and save each change.

import type { Budgeted } from './budget';

/** One limit as learned: its value, the step its next change takes, and which way it last went. */
export type LearnedLimit = { value: number; step: number; dir: -1 | 0 | 1 };
export type Learned = Record<Budgeted, LearnedLimit>;

/** What the defaults typically do, in the governor's terms (each meter against its budget at a limit of 1): Standard's
 *  median mean over eleven varied tracks at a quarter and three fifths of each (claude-5f's calibration, 2026-10-18,
 *  462 runs; precheck.ts), against which a meter stands out. New looks a minute: half the limit (Standard has none of
 *  its own). */
export const TYPICAL: Readonly<Record<Budgeted, number>> = {
  motion: 0.264,
  shapeChurn: 0.155,
  colourChurn: 0.036,
  zoomPumping: 0.068,
  looksPerMinute: 2,
};
/** The most the defaults do (the same runs' most), the other end of their usual range; new looks a minute: the limit. */
export const USUAL_MOST: Readonly<Record<Budgeted, number>> = {
  motion: 0.466,
  shapeChurn: 0.423,
  colourChurn: 0.354,
  zoomPumping: 0.351,
  looksPerMinute: 4,
};

export const FIRST_STEP = 0.1;
export const LEAST_STEP = 0.025;
/** Half of a limit's way back to where it started, in this many days. */
export const HALF_LIFE_DAYS = 30;
/** How far a limit may go from where it started: this share of it at least, this many times it at most. */
export const LEAST = 0.5;
export const MOST = 2;

/** Nothing learned: each limit where it starts. */
export function fresh(base: Readonly<Record<Budgeted, number>>): Learned {
  return Object.fromEntries((Object.keys(base) as Budgeted[]).map((k) => [k, { value: base[k], step: FIRST_STEP, dir: 0 }])) as Learned;
}

/** What was learned, `days` later: each limit that far back toward where it started (evenly in its logarithm). */
export function drifted(learned: Learned, base: Readonly<Record<Budgeted, number>>, days: number): Learned {
  const keep = 2 ** (-Math.max(0, days) / HALF_LIFE_DAYS);
  return Object.fromEntries(
    (Object.keys(base) as Budgeted[]).map((k) => {
      const l = learned[k] ?? { value: base[k], step: FIRST_STEP, dir: 0 };
      const value = Number.isFinite(base[k]) && l.value > 0 ? base[k] * (l.value / base[k]) ** keep : base[k];
      return [k, { ...l, value }];
    }),
  ) as Learned;
}

/** The 20 seconds before a thumb: each meter's mean against its budget at a limit of 1 (`budgetOf(1, wild, activity)`
 *  each reading), the governor's terms. */
export type Window = { relative: Partial<Record<Budgeted, number>> };

/** What a thumb does: the limits after it, and the one it changed (none where it only counts toward the fit).
 *  `held`: the limit the governor holds the picture back by now, if any. */
export function thumbed(
  learned: Learned,
  base: Readonly<Record<Budgeted, number>>,
  thumb: 'up' | 'down',
  window: Window,
  held: Budgeted | null,
): { learned: Learned; changed: { meter: Budgeted; from: number; to: number } | null } {
  let meter: Budgeted | null = null;
  let to = 0;
  if (thumb === 'down') {
    // The one that stood out most, if any stood out: past the defaults' typical, as a share of their usual range.
    let most = 0;
    for (const k of Object.keys(base) as Budgeted[]) {
      const v = window.relative[k];
      if (v === undefined || !Number.isFinite(base[k])) continue;
      const out = (v - TYPICAL[k]) / (USUAL_MOST[k] - TYPICAL[k]);
      if (out > most) {
        most = out;
        meter = k;
      }
    }
    if (meter) {
      const l = learned[meter];
      const step = l.dir === 1 ? Math.max(LEAST_STEP, l.step / 2) : l.step;
      to = Math.min(l.value, window.relative[meter]!) * (1 - step);
      learned = { ...learned, [meter]: { value: l.value, step, dir: -1 } };
    }
  } else if (held && Number.isFinite(base[held])) {
    meter = held;
    const l = learned[held];
    const step = l.dir === -1 ? Math.max(LEAST_STEP, l.step / 2) : l.step;
    to = l.value * (1 + step);
    learned = { ...learned, [held]: { value: l.value, step, dir: 1 } };
  }
  if (!meter) return { learned, changed: null };
  const from = learned[meter].value;
  const value = Math.min(base[meter] * MOST, Math.max(base[meter] * LEAST, to));
  return { learned: { ...learned, [meter]: { ...learned[meter], value } }, changed: { meter, from, to: value } };
}

/** The limits to hold the picture within, from what was learned. */
export const limitsOf = (learned: Learned): Record<Budgeted, number> =>
  Object.fromEntries(Object.entries(learned).map(([k, l]) => [k, l.value])) as Record<Budgeted, number>;
