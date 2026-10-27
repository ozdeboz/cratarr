// Genre territories on Insights' Map (plans.md, decided by the owner 2026-10-24; built 2026-10-26): each genre an area
// drawn around the playlists where it makes up much of the music, like a political map, so islands, holes and enclaves
// come out by themselves. The geometry is the accepted mock-up's (territories/geo.py): every playlist with at least
// SHARE of a genre adds a Gaussian bump (SIGMA wide) to that genre's field on a STEP grid, and the territory is the field's
// LEVEL contour (marching squares), its loops smoothed (closed Catmull-Rom). Six genres at most: the ones with most share
// among the playlists shown (the Map's filters, the owner).

const STEP = 4;
const SIGMA = 30;
/** A playlist counts toward a genre from this share of its music (the owner, 2026-10-27: 0.25 left 64 of 72 playlists
 *  in no territory, their genres spread over about five each; at 0.10, 62 are in one or more). */
const SHARE = 0.1;
const LEVEL = 0.5;
export const TERRITORIES = 6;

export type Territory = { genre: string; path: string; label: { x: number; y: number }; members: Set<number> };
type Pt = readonly [number, number];

/** A playlist's genre shares: the server's (`shares`), else its top genres shared out equally (a server before them). */
export function sharesOf(p: { genres: string[]; shares?: Record<string, number> }): Record<string, number> {
  if (p.shares) return p.shares;
  const n = p.genres.length;
  return Object.fromEntries(p.genres.map((g) => [g, 1 / n]));
}

/** The genres drawn: most share among the playlists where it counts (SHARE or more), at most TERRITORIES. */
export function topGenres(shares: Record<string, number>[]): string[] {
  const sum = new Map<string, number>();
  for (const s of shares) for (const [g, v] of Object.entries(s)) if (v >= SHARE) sum.set(g, (sum.get(g) ?? 0) + v);
  return [...sum]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, TERRITORIES)
    .map(([g]) => g);
}

/** Each genre's territory around the points (the graph's own coordinates). */
export function territories(points: Pt[], shares: Record<string, number>[], genres: string[]): Territory[] {
  if (!points.length) return [];
  const pad = 3 * SIGMA;
  const x0 = Math.min(...points.map((p) => p[0])) - pad;
  const y0 = Math.min(...points.map((p) => p[1])) - pad;
  const nx = Math.ceil((Math.max(...points.map((p) => p[0])) + pad - x0) / STEP) + 1;
  const ny = Math.ceil((Math.max(...points.map((p) => p[1])) + pad - y0) / STEP) + 1;
  return genres.map((genre) => {
    const members = new Set(points.map((_, i) => i).filter((i) => (shares[i][genre] ?? 0) >= SHARE));
    const f = new Float32Array(nx * ny);
    for (const i of members) {
      const [px, py] = points[i];
      const i0 = Math.max(0, Math.floor((px - pad - x0) / STEP));
      const i1 = Math.min(nx - 1, Math.ceil((px + pad - x0) / STEP));
      const j0 = Math.max(0, Math.floor((py - pad - y0) / STEP));
      const j1 = Math.min(ny - 1, Math.ceil((py + pad - y0) / STEP));
      for (let j = j0; j <= j1; j++) {
        const dy = y0 + j * STEP - py;
        for (let k = i0; k <= i1; k++) {
          const dx = x0 + k * STEP - px;
          f[j * nx + k] += Math.exp(-(dx * dx + dy * dy) / (2 * SIGMA * SIGMA));
        }
      }
    }
    const loops = contour(f, nx, ny, x0, y0);
    const largest = loops.reduce<Pt[] | null>((m, l) => (!m || l.length > m.length ? l : m), null);
    const label = largest
      ? { x: largest.reduce((s, p) => s + p[0], 0) / largest.length, y: largest.reduce((s, p) => s + p[1], 0) / largest.length }
      : { x: 0, y: 0 };
    return { genre, path: loops.map(smooth).join(' '), label, members };
  });
}

/** The field's LEVEL line as closed loops (marching squares, segments joined), every third point kept. */
function contour(f: Float32Array, nx: number, ny: number, x0: number, y0: number): Pt[][] {
  const at = (i: number, j: number): Pt => [x0 + i * STEP, y0 + j * STEP];
  const cross = (a: Pt, b: Pt, va: number, vb: number): Pt => {
    const t = vb !== va ? (LEVEL - va) / (vb - va) : 0.5;
    return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
  };
  const segs: [Pt, Pt][] = [];
  for (let j = 0; j < ny - 1; j++)
    for (let i = 0; i < nx - 1; i++) {
      const c = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
      const v = [f[j * nx + i], f[j * nx + i + 1], f[(j + 1) * nx + i + 1], f[(j + 1) * nx + i]];
      const inside = v.map((x) => x > LEVEL);
      if (inside.every(Boolean) || !inside.some(Boolean)) continue;
      const e: Pt[] = [];
      for (const [a, b] of [
        [0, 1],
        [1, 2],
        [2, 3],
        [3, 0],
      ])
        if (inside[a] !== inside[b]) e.push(cross(c[a], c[b], v[a], v[b]));
      for (let k = 0; k + 1 < e.length; k += 2) segs.push([e[k], e[k + 1]]);
    }
  const key = (p: Pt) => `${p[0].toFixed(2)},${p[1].toFixed(2)}`;
  const adj = new Map<string, Pt[]>();
  for (const [a, b] of segs) {
    adj.set(key(a), [...(adj.get(key(a)) ?? []), b]);
    adj.set(key(b), [...(adj.get(key(b)) ?? []), a]);
  }
  const used = new Set<string>();
  const loops: Pt[][] = [];
  for (const [a, b] of segs) {
    if (used.has(`${key(a)}|${key(b)}`)) continue;
    const loop: Pt[] = [a, b];
    used.add(`${key(a)}|${key(b)}`).add(`${key(b)}|${key(a)}`);
    for (;;) {
      const last = loop[loop.length - 1];
      const next = (adj.get(key(last)) ?? []).find((q) => !used.has(`${key(last)}|${key(q)}`));
      if (!next) break;
      used.add(`${key(last)}|${key(next)}`).add(`${key(next)}|${key(last)}`);
      if (key(next) === key(loop[0])) break;
      loop.push(next);
    }
    if (loop.length > 9) loops.push(loop.filter((_, k) => k % 3 === 0));
  }
  return loops;
}

/** A loop as a smooth closed path (Catmull-Rom as Béziers). */
function smooth(lp: Pt[]): string {
  const n = lp.length;
  const r = (x: number) => Math.round(x * 10) / 10;
  let d = `M${r(lp[0][0])} ${r(lp[0][1])}`;
  for (let i = 0; i < n; i++) {
    const [p0, p1, p2, p3] = [lp[(i - 1 + n) % n], lp[i], lp[(i + 1) % n], lp[(i + 2) % n]];
    d += `C${r(p1[0] + (p2[0] - p0[0]) / 6)} ${r(p1[1] + (p2[1] - p0[1]) / 6)} ${r(p2[0] - (p3[0] - p1[0]) / 6)} ${r(p2[1] - (p3[1] - p1[1]) / 6)} ${r(p2[0])} ${r(p2[1])}`;
  }
  return `${d}Z`;
}
