// How close playlists are, and which of those links to draw (Insights → Related). Pure functions; no drawing here.
//
// Measuring: each signal is turned into its rank among all pairs (0..1), so switching one on adds information in
// its own right instead of being drowned by another one's scale (genre is high almost everywhere in a library of one
// style). A pair's weight is the mean rank of the switched-on signals.
// Untangling: only a backbone is drawn: the strongest tree that reaches every playlist (maximum spanning tree), plus
// each pair that are among each other's K closest (mutual nearest neighbours). Measured on the reference library,
// K = 3 keeps about 1.1 links per playlist with every playlist reached; more brings the hairball back.
// Grab-bags weigh less: a playlist spread over many genres resembles a bit of everything and would sit in everyone's
// top 3 as a hub. Each playlist's focus (genre concentration, from the server) becomes its rank among the playlists,
// and a link counts for sqrt(m(a) * m(b)) of its weight, with m = FOCUS_FLOOR + (1 - FOCUS_FLOOR) * rank: the least
// focused playlist's links count half, the most focused's in full.
// Groups: label propagation over the backbone, for colour.

import type { Proximity, Signal } from './api';

export const SIGNALS: Signal[] = ['tracks', 'genre', 'artists'];
export const MUTUAL_K = 3;
export const FOCUS_FLOOR = 0.5;

export type Weights = Map<string, number>;
export const key = (a: number, b: number) => (a < b ? `${a}-${b}` : `${b}-${a}`);

/** Per signal, a function from a value to its rank among all pairs (pairs not sent count as 0). */
export function rankers(data: Proximity): Record<Signal, (v: number) => number> {
  const n = data.playlists.length;
  const pairs = (n * (n - 1)) / 2;
  const out = {} as Record<Signal, (v: number) => number>;
  for (const s of SIGNALS) {
    const vals = data.links.map((l) => l[s]).sort((x, y) => x - y);
    const zeros = pairs - vals.length;
    out[s] = (v) => {
      if (v <= 0) return 0;
      // Values at or below v, by binary search, plus the pairs not sent.
      let lo = 0;
      let hi = vals.length;
      while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if (vals[mid] <= v) lo = mid + 1;
        else hi = mid;
      }
      return (zeros + lo) / pairs;
    };
  }
  return out;
}

/** Each playlist's focus as its rank among the playlists, 0..1; without genre tags it counts as the middle. */
export function focusRanks(data: Proximity): number[] {
  const known = data.playlists.map((p) => p.focus).filter((f): f is number => f !== null).sort((a, b) => a - b);
  return data.playlists.map((p) => {
    if (p.focus === null || known.length < 2) return 0.5;
    const below = known.filter((f) => f < p.focus!).length;
    return below / (known.length - 1);
  });
}

/** Each pair's weight under the switched-on signals: the mean of their ranks, scaled down for grab-bags (see above);
 * pairs with nothing in common left out. */
export function weigh(data: Proximity, rank: Record<Signal, (v: number) => number>, on: Record<Signal, boolean>): Weights {
  const active = SIGNALS.filter((s) => on[s]);
  const w: Weights = new Map();
  if (!active.length) return w;
  const m = focusRanks(data).map((r) => FOCUS_FLOOR + (1 - FOCUS_FLOOR) * r);
  for (const l of data.links) {
    if (!active.some((s) => l[s] > 0)) continue;
    const mean = active.reduce((sum, s) => sum + rank[s](l[s]), 0) / active.length;
    w.set(key(l.a, l.b), mean * Math.sqrt(m[l.a] * m[l.b]));
  }
  return w;
}

/** The links worth drawing. `kin`: pairs among each other's K closest (mutual nearest neighbours), the real
 * kinship. `bridges`: links of the maximum spanning tree that are not kin, there only so every playlist is reached;
 * they are drawn slack and barely pull, so they don't glue unrelated groups together. */
export type Backbone = { kin: Weights; bridges: Weights };

export function backbone(n: number, w: Weights, k = MUTUAL_K): Backbone {
  const edges = [...w].map(([kk, v]) => {
    const [a, b] = kk.split('-').map(Number);
    return { a, b, v, kk };
  });
  edges.sort((x, y) => y.v - x.v);
  const tree: Weights = new Map();
  // Kruskal, strongest first.
  const parent = [...Array(n).keys()];
  const find = (x: number): number => (parent[x] === x ? x : (parent[x] = find(parent[x])));
  for (const e of edges) {
    const ra = find(e.a);
    const rb = find(e.b);
    if (ra !== rb) {
      parent[ra] = rb;
      tree.set(e.kk, e.v);
    }
  }
  // Mutual nearest neighbours.
  const near: Set<number>[] = Array.from({ length: n }, () => new Set());
  const count = new Array(n).fill(0);
  for (const e of edges) {
    if (count[e.a] < k) {
      near[e.a].add(e.b);
      count[e.a]++;
    }
    if (count[e.b] < k) {
      near[e.b].add(e.a);
      count[e.b]++;
    }
  }
  const kin: Weights = new Map();
  for (const e of edges) if (near[e.a].has(e.b) && near[e.b].has(e.a)) kin.set(e.kk, e.v);
  const bridges: Weights = new Map([...tree].filter(([kk]) => !kin.has(kk)));
  return { kin, bridges };
}

/** Each kin link's weight as its rank among the kin links, 0..1: the drawn links are all strong, so their raw
 * weights sit close together; ranked, strong and weak kinship look and pull differently. */
export function spread(kin: Weights): Weights {
  const sorted = [...kin.values()].sort((a, b) => a - b);
  const last = Math.max(1, sorted.length - 1);
  return new Map([...kin].map(([kk, v]) => [kk, sorted.indexOf(v) / last]));
}

/** Groups by weighted label propagation over the kin links; group 0 is the largest. Deterministic. */
export function groups(n: number, links: Weights): number[] {
  const adj: [number, number][][] = Array.from({ length: n }, () => []);
  for (const [kk, v] of links) {
    const [a, b] = kk.split('-').map(Number);
    adj[a].push([b, v]);
    adj[b].push([a, v]);
  }
  const label = [...Array(n).keys()];
  for (let round = 0; round < 30; round++) {
    let changed = false;
    for (let i = 0; i < n; i++) {
      if (!adj[i].length) continue;
      const score = new Map<number, number>();
      for (const [j, v] of adj[i]) score.set(label[j], (score.get(label[j]) ?? 0) + v);
      let best = label[i];
      let bestScore = score.get(best) ?? 0;
      for (const [l, s] of score) if (s > bestScore || (s === bestScore && l < best)) [best, bestScore] = [l, s];
      if (best !== label[i]) {
        label[i] = best;
        changed = true;
      }
    }
    if (!changed) break;
  }
  const size = new Map<number, number>();
  for (const l of label) size.set(l, (size.get(l) ?? 0) + 1);
  const order = [...size.keys()].sort((x, y) => size.get(y)! - size.get(x)! || x - y);
  const rank = new Map(order.map((l, i) => [l, i]));
  return label.map((l) => rank.get(l)!);
}
