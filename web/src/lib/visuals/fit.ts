// The governor's limits from the owner's ratings (decision 0046, amended). The owner rated pairs of behaviours (the
// rating page: A, then B, then A, B, Same or Neither), and each rating kept what both did (their meters' mean and
// most over twenty seconds). The limits are fitted all together: each meter either does not matter to the owner, or
// costs in proportion to how far it goes over its limit, and the owner takes the side that costs less (more surely the
// more it differs: a logistic choice). A meter gets a limit only where it explains the choices clearly better than
// leaving it out does (by more than a Bayesian information criterion's penalty), so a meter the ratings say nothing
// about keeps its default, and one meter's effect is not taken for another's (both differ in most pairs).
//
// Flashes are not fitted: WCAG's three a second is a safety rule, not a taste (meters.ts).

export type MeterSummary = Record<string, { mean: number; most: number }>;
export type RatedSide = { meters?: MeterSummary } & Record<string, unknown>;
export type Rating = { a: RatedSide; b: RatedSide; choice: string };

/** The meters fitted, and which of their summaries each is judged by (the mean: how much it does over the time; the most
 *  for new looks, which come one at a time). */
export const FITTED: Record<string, 'mean' | 'most'> = {
  motion: 'mean',
  shapeChurn: 'mean',
  colourChurn: 'mean',
  zoomPumping: 'mean',
  looksPerMinute: 'most',
};

/** How much a meter over its limit may cost, relative to its spread in the ratings. */
const WEIGHTS = [0.5, 1, 2, 4, 8];
/** Candidate limits: these shares of the levels the ratings saw. */
const QUANTILES = Array.from({ length: 19 }, (_, k) => (k + 1) / 20);
const SWEEPS = 4;

export type Fitted = {
  /** The limit fitted, or null where the ratings do not call for one (the default stands). */
  limit: number | null;
  /** The default it replaces. */
  standing: number;
  /** How much going over it costs (0: the meter does not matter), and how much better it explains the choices than
   *  leaving it out (log-likelihood). */
  weight: number;
  gain: number;
  /** The limits the ratings cannot tell from the one fitted (within two of its log-likelihood, its weight let free):
   *  how sure it is. A wide range wants more ratings, or pairs that differ along that meter. */
  range: [number, number] | null;
};

type Pair = { a: number[]; b: number[]; chose: 1 | -1 };

/** Each meter's limit from `ratings`, against `defaults` (the governor's standing limits). */
export function fitLimits(ratings: Rating[], defaults: Record<string, number>): Record<string, Fitted> {
  const meters = Object.keys(FITTED);
  const value = (side: RatedSide, m: string) => side.meters?.[m]?.[FITTED[m]];
  const pairs: Pair[] = [];
  for (const r of ratings) {
    if (r.choice !== 'a' && r.choice !== 'b') continue;
    const a = meters.map((m) => value(r.a, m));
    const b = meters.map((m) => value(r.b, m));
    if (a.some((x) => !Number.isFinite(x)) || b.some((x) => !Number.isFinite(x))) continue;
    pairs.push({ a: a as number[], b: b as number[], chose: r.choice === 'a' ? 1 : -1 });
  }
  const none = () => Object.fromEntries(meters.map((m) => [m, { limit: null, standing: defaults[m] ?? Infinity, weight: 0, gain: 0, range: null }]));
  if (pairs.length < 4) return none();

  // Each meter's spread in the ratings, so a weight means the same for every meter.
  const spread = meters.map((_, i) => {
    const all = pairs.flatMap((p) => [p.a[i], p.b[i]]).sort((x, y) => x - y);
    return Math.max(1e-6, all[Math.floor(all.length * 0.9)] - all[Math.floor(all.length * 0.1)]);
  });
  const candidates = meters.map((_, i) => {
    const all = pairs.flatMap((p) => [p.a[i], p.b[i]]).sort((x, y) => x - y);
    return [...new Set(QUANTILES.map((q) => all[Math.floor(q * (all.length - 1))]))];
  });
  const cost = (x: number[], w: number[], t: number[]) => x.reduce((c, v, i) => c + (w[i] * Math.max(0, v - t[i])) / spread[i], 0);
  const logLikelihood = (w: number[], t: number[]) =>
    pairs.reduce((ll, p) => {
      const z = p.chose * (cost(p.b, w, t) - cost(p.a, w, t));
      return ll - Math.log1p(Math.exp(-z));
    }, 0);

  const w = meters.map(() => 0);
  const t = meters.map((_, i) => candidates[i][0]);
  const penalty = Math.log(pairs.length); // BIC: half the log of n for each of a weight and a limit
  for (let sweep = 0; sweep < SWEEPS; sweep++) {
    for (let i = 0; i < meters.length; i++) {
      const without = [...w];
      without[i] = 0;
      let best = { ll: logLikelihood(without, t), w: 0, t: t[i], score: logLikelihood(without, t) };
      for (const wi of WEIGHTS) {
        for (const ti of candidates[i]) {
          const ww = [...w];
          const tt = [...t];
          ww[i] = wi;
          tt[i] = ti;
          const ll = logLikelihood(ww, tt);
          if (ll - penalty > best.score) best = { ll, w: wi, t: ti, score: ll - penalty };
        }
      }
      w[i] = best.w;
      t[i] = best.t;
    }
  }
  // Each limit found, refined between its neighbouring candidates (with its weight), the others held.
  for (let i = 0; i < meters.length; i++) {
    if (!w[i]) continue;
    const c = candidates[i];
    const k = c.indexOf(t[i]);
    const lo = c[Math.max(0, k - 1)];
    const hi = c[Math.min(c.length - 1, k + 1)];
    let best = { ll: logLikelihood(w, t), w: w[i], t: t[i] };
    for (let step = 0; step <= 20; step++) {
      for (const f of [0.6, 0.8, 1, 1.25, 1.6]) {
        const ww = [...w];
        const tt = [...t];
        tt[i] = lo + ((hi - lo) * step) / 20;
        ww[i] = w[i] * f;
        const ll = logLikelihood(ww, tt);
        if (ll > best.ll) best = { ll, w: ww[i], t: tt[i] };
      }
    }
    w[i] = best.w;
    t[i] = best.t;
  }
  const all = logLikelihood(w, t);
  // Each limit's range: the limits within two of the best log-likelihood, its weight let free, the others held.
  const rangeOf = (i: number): [number, number] => {
    const c = candidates[i];
    const [lo, hi] = [c[0], c[c.length - 1]];
    let inside: number[] = [];
    for (let step = 0; step <= 60; step++) {
      const ti = lo + ((hi - lo) * step) / 60;
      let best = -Infinity;
      for (const f of [0.25, 0.5, 1, 2, 4]) {
        const ww = [...w];
        const tt = [...t];
        ww[i] = w[i] * f;
        tt[i] = ti;
        best = Math.max(best, logLikelihood(ww, tt));
      }
      if (best >= all - 2) inside.push(ti);
    }
    if (!inside.length) inside = [t[i]];
    return [Math.min(...inside, t[i]), Math.max(...inside, t[i])];
  };
  return Object.fromEntries(
    meters.map((m, i) => {
      const without = [...w];
      without[i] = 0;
      const on = w[i] > 0;
      return [m, { limit: on ? t[i] : null, standing: defaults[m] ?? Infinity, weight: w[i], gain: on ? all - logLikelihood(without, t) : 0, range: on ? rangeOf(i) : null }];
    }),
  );
}
