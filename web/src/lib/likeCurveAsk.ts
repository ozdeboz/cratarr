// Sounds like's closeness curve as asked (lib/LikeCurve; 0.11, the owner: "Filtered curve in 0.11"): its seeds and,
// where the curve belongs to a crate, the crate's other conditions, so the curve counts only the tracks the crate
// picks among, as its preview counts them. With none, the GET says the same and every server has it; with some, the
// POST (0.11), and a server before it answers the GET instead (lib/api likeCurve). Pure: what is asked in, the
// GET's query or the POST's body out.
import type { Condition } from './api';

export type CurveAsk = {
  of: readonly string[];
  /** The crate's other conditions: a Sounds like among them, or one switched off, is passed over (withinOf). */
  within?: readonly Condition[];
  without?: readonly string[];
  fresh?: boolean;
  weights?: readonly number[];
  own?: boolean;
};

/** The conditions a curve counts within: those switched on, a Sounds like left out (its own curve; its cut moving
 *  under a drag must not ask again). */
export const withinOf = (conditions: readonly Condition[]): Condition[] =>
  conditions.filter((c) => c.enabled !== false && c.value.kind !== 'like');

const weightsOf = (w: readonly number[]) => w.map((x) => +x.toFixed(3));

/** The GET's query, as before 0.11. */
export function curveQuery(a: CurveAsk, source?: string): URLSearchParams {
  return new URLSearchParams({
    of: a.of.join(','),
    ...(source ? { source } : {}),
    ...(a.without?.length ? { without: a.without.join(',') } : {}),
    ...(a.fresh ? { fresh: 'true' } : {}),
    ...(a.own ? { own: 'true' } : {}),
    ...(a.weights?.length ? { weights: weightsOf(a.weights).join(',') } : {}),
  });
}

/** The POST's body (0.11): the same, its seeds as a list, with the conditions it counts within. */
export function curveBody(a: CurveAsk) {
  return {
    of: [...a.of],
    conditions: withinOf(a.within ?? []),
    ...(a.without?.length ? { without: [...a.without] } : {}),
    ...(a.fresh ? { fresh: true } : {}),
    ...(a.own ? { own: true } : {}),
    ...(a.weights?.length ? { weights: weightsOf(a.weights) } : {}),
  };
}

/** Whether the curve counts within conditions, and so needs the POST. */
export const asksWithin = (a: CurveAsk) => withinOf(a.within ?? []).length > 0;

/** What changes the curve, to ask again only on a change (not a Sounds like's cut, not a condition switched off). */
export const curveKey = (a: CurveAsk, source?: string) => JSON.stringify([source ?? null, curveBody(a)]);
