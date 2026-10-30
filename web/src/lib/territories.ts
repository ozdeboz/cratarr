// Genre territories on Insights' Map (plans.md, decided by the owner 2026-10-24; built 2026-10-26): each genre an area
// drawn around the playlists where it makes up much of the music, like a political map, so islands, holes and enclaves
// come out by themselves. The geometry is the accepted mock-up's (territories/geo.py): every playlist with at least
// SHARE of a genre adds a Gaussian bump (SIGMA wide) to that genre's field on a STEP grid, and the territory is the field's
// LEVEL contour (marching squares), its loops smoothed (closed Catmull-Rom). Which genres: the ones ticked in the Map's
// Genres sheet (the owner, 2026-10-27: up to MAX_GENRES), at first the TERRITORIES with most share among the playlists
// shown (the Map's filters).

const STEP = 4;
const SIGMA = 30;
/** A playlist counts toward a genre from this share of its music (the owner, 2026-10-27: 0.25 left 64 of 72 playlists
 *  in no territory, their genres spread over about five each; at 0.10, 62 are in one or more). */
const SHARE = 0.1;
const LEVEL = 0.5;
export const TERRITORIES = 6;
export const MAX_GENRES = 12;
/** The territories' colours, by a genre's place among those ticked: the Map's groups' nine (Tableau 10 without its grey),
 *  then three lighter ones. Twelve are not told apart by colour alone: each territory's name along its borders is. */
export const GENRE_PALETTE = ['#4e79a7', '#f28e2b', '#e15759', '#76b7b2', '#59a14f', '#edc948', '#b07aa1', '#ff9da7', '#9c755f', '#a0cbe8', '#ffbe7d', '#8cd17d'];

/** `edges`: its outer borders, its name written along each, inside (the owner, 2026-10-27: once more for each
 *  NAME_EVERY of border, so a large area carries it more often): the border going clockwise (`d`, the territory on its
 *  right) and the other way round (`back`), and where each name stands on it (`at`, 0 to 1 along `d`); where the border
 *  runs leftwards the name follows `back` (`flip`), so it always reads upright. */
export type EdgeName = { at: number; flip: boolean };
export type Territory = { genre: string; path: string; edges: { d: string; back: string; names: EdgeName[] }[]; members: Set<number> };
/** A name for each this much border (the graph's units). */
const NAME_EVERY = 160;
/** A border too short for a name is left unnamed (in kept points, every third of the grid's). */
const NAMED_EDGE = 16;
/** A name's size on the border (the graph's units; half a playlist's name, lib/ProximityGraph), and how wide a letter
 *  of it is about. */
export const NAME_SIZE = 5.5;
const LETTER = NAME_SIZE * 0.62;
/** How far apart two names must stay, and a name from a playlist's name's box. */
const CLEAR = 4;
/** Something a name must keep clear of (a playlist's name as drawn at rest): its box. */
export type Box = readonly [number, number, number, number];
type Pt = readonly [number, number];

/** A playlist's genre shares: the server's (`shares`), else its top genres shared out equally (a server before them). */
export function sharesOf(p: { genres: string[]; shares?: Record<string, number> }): Record<string, number> {
  if (p.shares) return p.shares;
  const n = p.genres.length;
  return Object.fromEntries(p.genres.map((g) => [g, 1 / n]));
}

/** Every genre that has a territory to draw, most share first (among the playlists where it counts, SHARE or more),
 *  and in how many playlists it counts. */
export function genreRanking(shares: Record<string, number>[]): { genre: string; playlists: number }[] {
  const sum = new Map<string, { share: number; playlists: number }>();
  for (const s of shares)
    for (const [g, v] of Object.entries(s))
      if (v >= SHARE) {
        const t = sum.get(g) ?? { share: 0, playlists: 0 };
        sum.set(g, { share: t.share + v, playlists: t.playlists + 1 });
      }
  return [...sum].sort((a, b) => b[1].share - a[1].share || a[0].localeCompare(b[0])).map(([genre, t]) => ({ genre, playlists: t.playlists }));
}

/** The genres drawn until some are ticked: the TERRITORIES with most share. */
export const topGenres = (shares: Record<string, number>[]): string[] => genreRanking(shares).slice(0, TERRITORIES).map((g) => g.genre);

/** Each genre's territory around the points (the graph's own coordinates). */
export function territories(points: Pt[], shares: Record<string, number>[], genres: string[], avoid: Box[] = []): Territory[] {
  if (!points.length) return [];
  const pad = 3 * SIGMA;
  const x0 = Math.min(...points.map((p) => p[0])) - pad;
  const y0 = Math.min(...points.map((p) => p[1])) - pad;
  const nx = Math.ceil((Math.max(...points.map((p) => p[0])) + pad - x0) / STEP) + 1;
  const ny = Math.ceil((Math.max(...points.map((p) => p[1])) + pad - y0) / STEP) + 1;
  const drawn = genres.map((genre) => {
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
    // The field just right of a border's first step: inside the territory, or the loop turns round.
    const at = (x: number, y: number) => f[Math.min(ny - 1, Math.max(0, Math.round((y - y0) / STEP))) * nx + Math.min(nx - 1, Math.max(0, Math.round((x - x0) / STEP)))];
    const outer = loops
      .map((lp) => {
        const [a, b] = [lp[0], lp[1]];
        const len = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
        const [mx, my] = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
        const right = at(mx - ((b[1] - a[1]) / len) * STEP, my + ((b[0] - a[0]) / len) * STEP);
        return right > LEVEL ? lp : [...lp].reverse();
      })
      // The territory on the right and going clockwise: an outer border (a hole's goes the other way round).
      .filter((lp) => lp.length >= NAMED_EDGE && area(lp) > 0)
      .map((loop) => {
        const top = loop.reduce((t, p, i) => (p[1] < loop[t][1] ? i : t), 0);
        return [...loop.slice(top), ...loop.slice(0, top)];
      });
    return { genre, path: loops.map(smooth).join(' '), outer, members };
  });
  // The names, every territory's at once, so they keep clear of each other and of the playlists' names.
  const placed: Pt[][] = [];
  return drawn.map(({ genre, path, outer, members }) => ({
    genre,
    path,
    members,
    edges: outer.map((lp) => ({ d: smooth(lp), back: smooth([...lp].reverse()), names: nameSpots(lp, genre, avoid, placed) })),
  }));
}

/** Where a border carries its name: once for each NAME_EVERY of it, each as near its even share of the border as it
 *  can stand clear of the names placed already and of `avoid` (searched to either side, up to half its share; none
 *  there: left out). `placed` gains the footprints taken. */
function nameSpots(lp: Pt[], genre: string, avoid: Box[], placed: Pt[][]): EdgeName[] {
  const along = [0];
  for (let i = 1; i <= lp.length; i++) along.push(along[i - 1] + Math.hypot(lp[i % lp.length][0] - lp[i - 1][0], lp[i % lp.length][1] - lp[i - 1][1]));
  const round = along[lp.length];
  const wide = genre.length * LETTER;
  if (wide > round * 0.8) return [];
  /** The point at a share of the way round, and the border's way there. */
  const point = (u: number) => {
    const s = (((u % 1) + 1) % 1) * round;
    const i = Math.max(0, along.findIndex((x) => x > s) - 1);
    const [p, q] = [lp[i], lp[(i + 1) % lp.length]];
    const seg = along[i + 1] - along[i] || 1;
    const t = (s - along[i]) / seg;
    return { x: p[0] + (q[0] - p[0]) * t, y: p[1] + (q[1] - p[1]) * t, dx: (q[0] - p[0]) / seg, dy: (q[1] - p[1]) / seg };
  };
  /** What a name covers from `u` on: points along it, a little inside the border. */
  const footprint = (u: number): Pt[] => {
    const out: Pt[] = [];
    for (let s = 0; s <= wide; s += 2) {
      const p = point(u + s / round);
      out.push([p.x - p.dy * (NAME_SIZE / 2 + 1.5), p.y + p.dx * (NAME_SIZE / 2 + 1.5)]);
    }
    return out;
  };
  const clear = (f: Pt[]) =>
    f.every(([x, y]) => avoid.every(([x0, y0, x1, y1]) => x < x0 - CLEAR || x > x1 + CLEAR || y < y0 - CLEAR || y > y1 + CLEAR)) &&
    placed.every((g) => f.every(([x, y]) => g.every(([gx, gy]) => Math.abs(gx - x) > CLEAR + NAME_SIZE / 2 || Math.abs(gy - y) > CLEAR + NAME_SIZE / 2)));
  const count = Math.max(1, Math.floor(round / NAME_EVERY));
  const share = 1 / count;
  const step = 2 / round;
  const out: EdgeName[] = [];
  for (let m = 0; m < count; m++) {
    const want = 0.02 + m * share;
    for (let tried = 0; tried * step <= share / 2; tried++) {
      // Whole on the border's path, which starts and ends at its top: not running past its end.
      const fits = (u: number) => u >= 0 && u + wide / round <= 0.99;
      const at = [want + tried * step, want - tried * step].find((u) => fits(u) && clear(footprint(u)));
      if (at === undefined) continue;
      const u = at;
      placed.push(footprint(u));
      // Upright: where the border runs leftwards (taken about the name's middle), along it the other way round.
      out.push({ at: u, flip: point(u + wide / 2 / round).dx < 0 });
      break;
    }
  }
  return out;
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

/** A loop's area, positive going clockwise on screen (y down). */
function area(lp: Pt[]): number {
  let s = 0;
  for (let i = 0; i < lp.length; i++) {
    const [p, q] = [lp[i], lp[(i + 1) % lp.length]];
    s += p[0] * q[1] - q[0] * p[1];
  }
  return s / 2;
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
