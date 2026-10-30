// How close playlists are, and which of those links to draw (Insights → Related). Pure functions; no drawing here.
//
// Measuring: each signal is turned into its rank among all pairs (0..1), so switching one on adds information in
// its own right instead of being drowned by another one's scale (genre is high almost everywhere in a library of one
// style). A pair's weight is the mean rank of the switched-on signals.
// Untangling: only a backbone is drawn. Solid (kin): each pair among each other's K closest (mutual nearest
// neighbours), plus every playlist's own closest link, so no playlist is left without a solid line. Dashed (bridges):
// the links of the strongest tree that reaches every playlist (maximum spanning tree) that are not kin.
// Measured on the reference library (66 playlists, 6 layout seeds): mutual top-3 alone left 8 playlists without a
// solid line, 7 of them grab-bags (a playlist spread over many genres is nobody's top 3); with each one's closest link
// added, none, 69 solid lines, 15 crossings. Top-4 or two closest links each only add clutter (40-60 crossings).
// Grab-bags are not weighed down: doing so (an earlier version) cost them their solid lines; their focus is shown in
// the panel only.
// Groups (colour and a gentle pull): Louvain modularity over each playlist's GROUP_NEIGHBOURS strongest links, not
// only the drawn ones, so every playlist belongs somewhere. Measured on the reference library (66 playlists, current
// scoring): 6 neighbours at resolution 2.0 gives 9 groups (10 10 8 8 8 6 6 6 4), members as alike as the best setting
// (similarity silhouette 0.35); resolution 1.2 gave only 6 broad groups (0.33). More groups sit less together on
// screen for the same pull, so the pull went up (see ProximityGraph).

import type { Proximity, Signal } from './api';

export const SIGNALS: Signal[] = ['tracks', 'genre', 'artists'];
export const MUTUAL_K = 3;
export const GROUP_NEIGHBOURS = 6;
export const GROUP_RESOLUTION = 2.0;

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

/** Each pair's weight under the switched-on signals: the mean of their ranks; pairs with nothing in common left
 * out. */
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

/** The links worth drawing (see above). `kin`: mutual K closest plus each playlist's own closest, drawn solid.
 * `bridges`: tree links that are not kin, there only so every playlist is reached; drawn slack and barely pulling,
 * so they don't glue unrelated groups together. */
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
  // Edges are sorted strongest first, so a playlist's first edge is its own closest link.
  const seen = new Set<number>();
  for (const e of edges) {
    for (const x of [e.a, e.b]) {
      if (!seen.has(x)) {
        seen.add(x);
        kin.set(e.kk, e.v);
      }
    }
  }
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

/** The K strongest links of every playlist (a pair counts when it is in either one's K), for grouping. */
export function nearest(n: number, w: Weights, k: number): Weights {
  const by: { kk: string; v: number }[][] = Array.from({ length: n }, () => []);
  for (const [kk, v] of w) for (const x of kk.split('-').map(Number)) by[x].push({ kk, v });
  const out: Weights = new Map();
  for (const list of by) for (const { kk, v } of list.sort((x, y) => y.v - x.v).slice(0, k)) out.set(kk, v);
  return out;
}

/** Groups by modularity (Louvain: move single points to the neighbouring group that gains most, then merge groups
 * into points and repeat). `resolution` above 1 makes more, smaller groups. Group 0 is the largest; deterministic. */
export function communities(n: number, links: Weights, resolution = 1): number[] {
  // Graph of the current level: adjacency with weights; self-loops hold weight inside a merged point.
  let adj: Map<number, number>[] = Array.from({ length: n }, () => new Map());
  for (const [kk, v] of links) {
    const [a, b] = kk.split('-').map(Number);
    adj[a].set(b, (adj[a].get(b) ?? 0) + v);
    adj[b].set(a, (adj[b].get(a) ?? 0) + v);
  }
  let member = [...Array(n).keys()];
  for (;;) {
    const size = adj.length;
    const degree = adj.map((m, i) => [...m].reduce((s, [j, v]) => s + (j === i ? 2 * v : v), 0));
    const twoM = degree.reduce((s, d) => s + d, 0);
    if (twoM === 0) break;
    const comm = [...Array(size).keys()];
    const total = [...degree];
    let movedAny = false;
    for (let moved = true, rounds = 0; moved && rounds < 50; rounds++) {
      moved = false;
      for (let i = 0; i < size; i++) {
        const own = comm[i];
        total[own] -= degree[i];
        const towards = new Map<number, number>();
        for (const [j, v] of adj[i]) if (j !== i) towards.set(comm[j], (towards.get(comm[j]) ?? 0) + v);
        const gain = (c: number) => (towards.get(c) ?? 0) - (resolution * total[c] * degree[i]) / twoM;
        let best = own;
        let bestGain = gain(own);
        for (const c of [...towards.keys()].sort((x, y) => x - y)) {
          const g = gain(c);
          if (g > bestGain + 1e-12) [best, bestGain] = [c, g];
        }
        comm[i] = best;
        total[best] += degree[i];
        if (best !== own) moved = movedAny = true;
      }
    }
    if (!movedAny) break;
    // Merge each group into one point of the next level.
    const ids = new Map<number, number>();
    for (const c of comm) if (!ids.has(c)) ids.set(c, ids.size);
    member = member.map((m) => ids.get(comm[m])!);
    const next: Map<number, number>[] = Array.from({ length: ids.size }, () => new Map());
    for (let i = 0; i < size; i++) {
      for (const [j, v] of adj[i]) {
        if (j < i) continue;
        const [a, b] = [ids.get(comm[i])!, ids.get(comm[j])!];
        if (a === b) next[a].set(a, (next[a].get(a) ?? 0) + v);
        else {
          next[a].set(b, (next[a].get(b) ?? 0) + v);
          next[b].set(a, (next[b].get(a) ?? 0) + v);
        }
      }
    }
    adj = next;
  }
  const size = new Map<number, number>();
  for (const g of member) size.set(g, (size.get(g) ?? 0) + 1);
  const order = [...size.keys()].sort((x, y) => size.get(y)! - size.get(x)! || x - y);
  const rank = new Map(order.map((g, i) => [g, i]));
  return member.map((g) => rank.get(g)!);
}
