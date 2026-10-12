// How close playlists are, and which of those links to draw (Insights → Related). Pure functions; no drawing here.
//
// Measuring: each signal is turned into its rank among all pairs (0..1), so switching one on adds information in
// its own right instead of being drowned by another one's scale (genre is high almost everywhere in a library of one
// style). A pair's weight is the mean rank of the switched-on signals.
// Untangling: only a backbone is drawn: the strongest tree that reaches every playlist (maximum spanning tree), plus
// each pair that are among each other's K closest (mutual nearest neighbours). Measured on the reference library,
// K = 3 keeps about 1.1 links per playlist with every playlist reached; more brings the hairball back.
// Groups: label propagation over the backbone, for colour.

import type { Proximity, Signal } from './api';

export const SIGNALS: Signal[] = ['tracks', 'genre', 'artists'];
export const MUTUAL_K = 3;

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

/** Each pair's weight under the switched-on signals: the mean of their ranks; pairs with nothing in common left out. */
export function weigh(data: Proximity, rank: Record<Signal, (v: number) => number>, on: Record<Signal, boolean>): Weights {
  const active = SIGNALS.filter((s) => on[s]);
  const w: Weights = new Map();
  if (!active.length) return w;
  for (const l of data.links) {
    if (!active.some((s) => l[s] > 0)) continue;
    w.set(key(l.a, l.b), active.reduce((sum, s) => sum + rank[s](l[s]), 0) / active.length);
  }
  return w;
}

/** The links worth drawing: maximum spanning tree plus mutual K-nearest neighbours. */
export function backbone(n: number, w: Weights, k = MUTUAL_K): Weights {
  const edges = [...w].map(([kk, v]) => {
    const [a, b] = kk.split('-').map(Number);
    return { a, b, v, kk };
  });
  edges.sort((x, y) => y.v - x.v);
  const keep: Weights = new Map();
  // Kruskal, strongest first.
  const parent = [...Array(n).keys()];
  const find = (x: number): number => (parent[x] === x ? x : (parent[x] = find(parent[x])));
  for (const e of edges) {
    const ra = find(e.a);
    const rb = find(e.b);
    if (ra !== rb) {
      parent[ra] = rb;
      keep.set(e.kk, e.v);
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
  for (const e of edges) if (near[e.a].has(e.b) && near[e.b].has(e.a)) keep.set(e.kk, e.v);
  return keep;
}

/** Groups by weighted label propagation over the drawn links; group 0 is the largest. Deterministic. */
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
