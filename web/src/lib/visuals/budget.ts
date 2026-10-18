// The governor's budgets (decision 0048): the meters that have one, and what a limit allows as the Calm ↔ Wild setting
// and the music have it. The one scale the governor, the dice's precheck, the ratings and the thumbs all judge on: a
// meter over its budget at Wild's middle (`budgetOf(1, 0.5, activity)`) is how far it is from a limit, whatever the
// music was doing. Apart from governor.ts so the precheck (whose limits the governor stands on) can take it too.

/** The meters with a budget. */
export const BUDGETED = ['motion', 'shapeChurn', 'colourChurn', 'zoomPumping', 'looksPerMinute'] as const;
export type Budgeted = (typeof BUDGETED)[number];

/** A budget as the Calm ↔ Wild setting (0..1: half as much to twice, as much at its middle) and the music (its
 *  activity now, 0..1: half as much when quiet, one and a half times when loud) have it. */
export const budgetOf = (limit: number, wild: number, activity: number) => limit * 2 ** (2 * (wild - 0.5)) * (0.5 + Math.min(1, Math.max(0, activity)));
