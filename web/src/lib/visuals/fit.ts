// The governor's limits from the owner's ratings (decision 0046, amended). The owner rated pairs of behaviours (the
// rating page: A, then B, then A, B, Same or Neither), and each rating kept what both did over twenty seconds, on the
// governor's own scale (`relative`: each budgeted meter over its budget at the music's activity then, budget.ts), so a
// limit fitted here is one the governor holds as it is. The limits are fitted all together: each meter either does not
// matter to the owner, or costs in proportion to how far it goes over its limit, and the owner takes the side that costs
// less (more surely the more it differs), or calls them the same (more likely the closer they are: Davidson's model of
// paired choices with ties, its tie rate fitted too). A meter gets a limit only where it explains the answers clearly
// better than leaving it out does (by more than a Bayesian information criterion's penalty), so a meter the ratings say
// nothing about keeps its default, and one meter's effect is not taken for another's (both differ in most pairs).
// Neither says nothing about which was better, only that both were off: counted, not fitted.
//
// Flashes are not fitted: three a second over a quarter of the screen is a safety rule, not a taste (meters.ts).

export type Summary = Record<string, { mean: number; most: number }>;
export type RatedSide = { relative?: Summary } & Record<string, unknown>;
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
/** Candidate tie rates (Davidson's ν: 0, none; 1, a tie as likely as either side when they cost the same). */
const TIES = [0, 0.05, 0.1, 0.2, 0.4, 0.8, 1.6, 3.2];
const SWEEPS = 4;

export type Fitted = {
  /** The limit fitted, or null where the ratings do not call for one (the default stands). */
  limit: number | null;
  /** The default it replaces. */
  standing: number;
  /** How much going over it costs (0: the meter does not matter), and how much better it explains the answers than
   *  leaving it out (log-likelihood). */
  weight: number;
  gain: number;
  /** The limits the ratings cannot tell from the one fitted (within two of its log-likelihood, its weight let free):
   *  how sure it is. A wide range wants more ratings, or pairs that differ along that meter. */
  range: [number, number] | null;
};

export type Fit = {
  meters: Record<string, Fitted>;
  /** The tie rate fitted (ν). */
  tie: number;
  /** The answers fitted (A or B, Same), Neither's (counted only), and the ratings left out (not on the governor's
   *  scale, or a meter missing). */
  used: { chosen: number; same: number; neither: number; skipped: number };
};

type Pair = { a: number[]; b: number[]; chose: 1 | -1 | 0 };

/** Each meter's limit from `ratings`, against `defaults` (the governor's standing limits). */
export function fitLimits(ratings: Rating[], defaults: Record<string, number>): Fit {
  const meters = Object.keys(FITTED);
  const value = (side: RatedSide, m: string) => side.relative?.[m]?.[FITTED[m]];
  const pairs: Pair[] = [];
  const used = { chosen: 0, same: 0, neither: 0, skipped: 0 };
  for (const r of ratings) {
    if (r.choice === 'neither') {
      used.neither++;
      continue;
    }
    if (r.choice !== 'a' && r.choice !== 'b' && r.choice !== 'same') {
      used.skipped++;
      continue;
    }
    const a = meters.map((m) => value(r.a, m));
    const b = meters.map((m) => value(r.b, m));
    if (a.some((x) => !Number.isFinite(x)) || b.some((x) => !Number.isFinite(x))) {
      used.skipped++;
      continue;
    }
    pairs.push({ a: a as number[], b: b as number[], chose: r.choice === 'a' ? 1 : r.choice === 'b' ? -1 : 0 });
    if (r.choice === 'same') used.same++;
    else used.chosen++;
  }
  const none = (): Fit => ({
    meters: Object.fromEntries(meters.map((m) => [m, { limit: null, standing: defaults[m] ?? Infinity, weight: 0, gain: 0, range: null }])),
    tie: 0,
    used,
  });
  if (used.chosen < 4) return none();

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
  // Davidson: A, B and Same in proportion to e^-cost(A), e^-cost(B) and ν e^-(cost(A) + cost(B))/2 (taken relative to
  // the cheaper side, so nothing overflows).
  const logLikelihood = (w: number[], t: number[], nu: number) =>
    pairs.reduce((ll, p) => {
      const ca = cost(p.a, w, t);
      const cb = cost(p.b, w, t);
      const low = Math.min(ca, cb);
      const ea = Math.exp(low - ca);
      const eb = Math.exp(low - cb);
      const et = nu * Math.sqrt(ea * eb);
      const chosen = p.chose === 1 ? ea : p.chose === -1 ? eb : et;
      return ll + Math.log(Math.max(1e-300, chosen / (ea + eb + et)));
    }, 0);

  const w = meters.map(() => 0);
  const t = meters.map((_, i) => candidates[i][0]);
  let nu = used.same ? TIES[3] : 0;
  const bestTie = () => {
    if (!used.same) return 0;
    let best = { nu, ll: -Infinity };
    for (const n of TIES.slice(1)) {
      const ll = logLikelihood(w, t, n);
      if (ll > best.ll) best = { nu: n, ll };
    }
    return best.nu;
  };
  const penalty = Math.log(pairs.length); // BIC: half the log of n for each of a weight and a limit
  for (let sweep = 0; sweep < SWEEPS; sweep++) {
    for (let i = 0; i < meters.length; i++) {
      const without = [...w];
      without[i] = 0;
      const left = logLikelihood(without, t, nu);
      let best = { ll: left, w: 0, t: t[i], score: left };
      for (const wi of WEIGHTS) {
        for (const ti of candidates[i]) {
          const ww = [...w];
          const tt = [...t];
          ww[i] = wi;
          tt[i] = ti;
          const ll = logLikelihood(ww, tt, nu);
          if (ll - penalty > best.score) best = { ll, w: wi, t: ti, score: ll - penalty };
        }
      }
      w[i] = best.w;
      t[i] = best.t;
    }
    nu = bestTie();
  }
  // Each limit found, refined between its neighbouring candidates (with its weight), the others held.
  for (let i = 0; i < meters.length; i++) {
    if (!w[i]) continue;
    const c = candidates[i];
    const k = c.indexOf(t[i]);
    const lo = c[Math.max(0, k - 1)];
    const hi = c[Math.min(c.length - 1, k + 1)];
    let best = { ll: logLikelihood(w, t, nu), w: w[i], t: t[i] };
    for (let step = 0; step <= 20; step++) {
      for (const f of [0.6, 0.8, 1, 1.25, 1.6]) {
        const ww = [...w];
        const tt = [...t];
        tt[i] = lo + ((hi - lo) * step) / 20;
        ww[i] = w[i] * f;
        const ll = logLikelihood(ww, tt, nu);
        if (ll > best.ll) best = { ll, w: ww[i], t: tt[i] };
      }
    }
    w[i] = best.w;
    t[i] = best.t;
  }
  nu = bestTie();
  const all = logLikelihood(w, t, nu);
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
        best = Math.max(best, logLikelihood(ww, tt, nu));
      }
      if (best >= all - 2) inside.push(ti);
    }
    if (!inside.length) inside = [t[i]];
    return [Math.min(...inside, t[i]), Math.max(...inside, t[i])];
  };
  return {
    meters: Object.fromEntries(
      meters.map((m, i) => {
        const without = [...w];
        without[i] = 0;
        const on = w[i] > 0;
        return [m, { limit: on ? t[i] : null, standing: defaults[m] ?? Infinity, weight: w[i], gain: on ? all - logLikelihood(without, t, nu) : 0, range: on ? rangeOf(i) : null }];
      }),
    ),
    tie: nu,
    used,
  };
}
