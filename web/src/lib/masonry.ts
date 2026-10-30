// Discover's masonry around an artist played (the owner, 2026-10-25, 0.9.2): as wide as the page allows, two rows at
// least, every place filled. The artist's tile lies flat (two places along a row, with their photo) in the middle; the
// suggestions are packed outward from it, the strongest nearest, as 1x1, 2x1 and 1x2 tiles (no larger: the owner's
// shapes), a third of them at most of two places, what plays on Bandcamp first. A shuffler, seeded by the artist so a layout stays put, varies
// which releases take two places and how they lie, and no two flat tiles lie exactly on one another
// nor two upright ones stand exactly side by side (a brick's bond, not a straight grid: "four alike two-place tiles"
// were "totally unorganic"). Each artist gets the largest grid its suggestions fill completely; a second artist takes
// the width left over. Pure: places in, a grid out.

/** A suggestion as the masonry reads it: `plays`, it is among the first to take two places. */
export type Placeable = { key: string; plays: boolean };
/** A tile's place: its column and row (from 0) and its width and height in places. */
export type Tile = { key: string; col: number; row: number; w: number; h: number };
/** `partner`: the second artist's tile, one place, where there is one. */
export type Grid = { cols: number; rows: number; seed: Tile; partner: Tile | null; tiles: Tile[] };

/** The most rows: three, four where the page has only three columns (a phone), so it grows down instead. */
export const rowsFor = (cols: number) => (cols <= 3 ? 4 : 3);

/** The artist's tile: flat, two places along a row, in the middle (the top row where there are two); with a second
 *  artist (the owner, 2026-10-25: the library's artist closest to the first, "only a 1x1 tile", "always one between the
 *  first and the second") the two in the middle together, a place apart. */
function seedsOf(cols: number, rows: number, two: boolean): { seed: Tile; partner: Tile | null } {
  const row = rows === 2 ? 0 : Math.floor((rows - 1) / 2);
  if (!two) return { seed: { key: 'seed', col: Math.floor((cols - 2) / 2), row, w: 2, h: 1 }, partner: null };
  const col = Math.floor((cols - 4) / 2);
  return { seed: { key: 'seed', col, row, w: 2, h: 1 }, partner: { key: 'partner', col: col + 3, row, w: 1, h: 1 } };
}

type Sized = { key: string; size: 1 | 2 };

/** A small seeded random (mulberry32): the same artist, the same layout. */
function random(seed: string) {
  let h = 1779033703;
  for (const ch of seed) h = Math.imul(h ^ ch.charCodeAt(0), 3432918353);
  return () => {
    h = (h + 0x6d2b79f5) | 0;
    let t = Math.imul(h ^ (h >>> 15), 1 | h);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Which suggestions fill `free` places, and which of them take two. Null where they cannot fill it. */
function pick<T extends Placeable>(items: T[], free: number, rnd: () => number): { chosen: T[]; sizes: Sized[] } | null {
  // As many as fit, now and then one fewer and one more of two places (the owner, 2026-10-25: "shuffle between one or
  // two tiles per release", for a varied pattern); a third at most of two places.
  const most = Math.min(items.length, free);
  const fits = (m: number) => m >= 1 && free - m <= Math.ceil(m / 3);
  if (!fits(most)) return null;
  const m = fits(most - 1) && rnd() < 0.35 ? most - 1 : most;
  const chosen = items.slice(0, m);
  // Which grow: what plays first (the owner, 2026-10-25: "the playable releases should get priority"), then the rest,
  // each shuffled.
  const shuffled = (xs: T[]) => [...xs].sort(() => rnd() - 0.5);
  const order = [...shuffled(chosen.filter((x) => x.plays)), ...shuffled(chosen.filter((x) => !x.plays))];
  const grow = new Set(order.slice(0, free - m).map((x) => x.key));
  return { chosen, sizes: chosen.map((x) => ({ key: x.key, size: grow.has(x.key) ? 2 : 1 })) };
}

/** The tiles around the seed, nearest first (a little shuffled): the two-place ones first, along the row above or
 *  below the seed and upright beside it where they can, never exactly on a flat one nor beside an upright one of the
 *  same rows; then the rest in the places left. Null where a two-place tile finds no room. */
function pack(cols: number, rows: number, seeds: Tile[], items: Sized[], rnd: () => number): Tile[] | null {
  const used = Array.from({ length: rows }, () => Array<boolean>(cols).fill(false));
  const mark = (t: Tile) => {
    for (let r = t.row; r < t.row + t.h; r++) for (let c = t.col; c < t.col + t.w; c++) used[r][c] = true;
  };
  seeds.forEach(mark);
  // How far a place is from the nearer artist, and that artist's middle.
  const middle = (t: Tile) => [t.col + (t.w - 1) / 2, t.row + (t.h - 1) / 2] as const;
  const from = (t: Tile, c: number, r: number) => {
    const [x, y] = middle(t);
    return Math.max(Math.abs(c - x), Math.abs(r - y)) * 100 + (c - x) ** 2 + (r - y) ** 2;
  };
  const nearest = (c: number, r: number) => seeds.reduce((a, b) => (from(b, c, r) < from(a, c, r) ? b : a));
  const far = (c: number, r: number) => from(nearest(c, r), c, r);
  const places: [number, number][] = [];
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) places.push([c, r]);
  const jitter = new Map(places.map(([c, r]) => [`${c},${r}`, rnd() * 60]));
  const near = (c: number, r: number) => far(c, r) + jitter.get(`${c},${r}`)!;
  places.sort((a, b) => near(...a) - near(...b));
  const free = (c: number, r: number) => c >= 0 && r >= 0 && c < cols && r < rows && !used[r][c];
  const fits = (t: Tile) => {
    for (let r = t.row; r < t.row + t.h; r++) for (let c = t.col; c < t.col + t.w; c++) if (!free(c, r)) return false;
    return true;
  };
  const tiles: Tile[] = [];
  const flats = seeds.filter((t) => t.w === 2);
  const uprights: Tile[] = [];
  // A brick's bond: a flat tile not exactly on or under another, an upright one not exactly beside another.
  const bonds = (t: Tile) =>
    t.w === 2
      ? !flats.some((f) => f.col === t.col && Math.abs(f.row - t.row) === 1)
      : !uprights.some((u) => u.row === t.row && Math.abs(u.col - t.col) === 1);
  for (const it of items.filter((x) => x.size === 2)) {
    let put: Tile | null = null;
    for (const [c, r] of places) {
      if (!free(c, r)) continue;
      const n = nearest(c, r);
      const [cx, cy] = middle(n);
      const along = Math.abs(r - cy) >= Math.abs(c - cx) - (n.w - 1) / 2;
      const flat: Tile[] = [{ key: it.key, col: c, row: r, w: 2, h: 1 }, { key: it.key, col: c - 1, row: r, w: 2, h: 1 }];
      const upright: Tile[] = [{ key: it.key, col: c, row: r, w: 1, h: 2 }, { key: it.key, col: c, row: r - 1, w: 1, h: 2 }];
      // Mostly as the place says, sometimes the other way: the shuffle.
      const flatFirst = along === rnd() < 0.8;
      put = (flatFirst ? [...flat, ...upright] : [...upright, ...flat]).find((t) => fits(t) && bonds(t)) ?? null;
      if (put) break;
    }
    if (!put) return null;
    mark(put);
    tiles.push(put);
    (put.w === 2 ? flats : uprights).push(put);
  }
  const singles = items.filter((x) => x.size === 1);
  for (const [c, r] of places) {
    if (!free(c, r)) continue;
    const it = singles.shift();
    if (!it) return null;
    used[r][c] = true;
    tiles.push({ key: it.key, col: c, row: r, w: 1, h: 1 });
  }
  return singles.length ? null : tiles;
}

/** Shuffles tried on a size before it is given up. */
const TRIES = 24;

/** The largest grid of at most `maxCols` columns (three at least, four with a second artist; two rows at least) that
 *  `items` fill completely, in their order, else null; of two as large, the one showing more suggestions, then three rows before two. */
export function layout<T extends Placeable>(items: T[], maxCols: number, seedName = '', two = false): { grid: Grid; items: T[] } | null {
  let best: { grid: Grid; items: T[] } | null = null;
  const area = (g: Grid) => g.cols * g.rows;
  for (let cols = two ? 4 : 3; cols <= maxCols; cols++) {
    for (let rows = 2; rows <= rowsFor(maxCols); rows++) {
      const { seed, partner } = seedsOf(cols, rows, two);
      const seeds = partner ? [seed, partner] : [seed];
      const rnd = random(`${seedName}|${cols}x${rows}`);
      for (let i = 0; i < TRIES; i++) {
        const p = pick(items, cols * rows - seeds.reduce((n, t) => n + t.w * t.h, 0), rnd);
        if (!p) break;
        const tiles = pack(cols, rows, seeds, p.sizes, rnd);
        if (!tiles) continue;
        const grid = { cols, rows, seed, partner, tiles };
        const better =
          !best ||
          area(grid) > area(best.grid) ||
          (area(grid) === area(best.grid) && (p.chosen.length > best.items.length || (p.chosen.length === best.items.length && rows === 3 && best.grid.rows !== 3)));
        if (better) best = { grid, items: p.chosen };
        break;
      }
    }
  }
  return best;
}
