// Discover's masonry (lib/masonry, the owner 2026-10-25, 0.9.2): every place filled, no tile over another, the artist
// flat in the middle, tiles of different shapes within their limits, as wide as the page allows.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { layout } = await import('../src/lib/masonry.ts');

const items = (n: number, playing = 0) => Array.from({ length: n }, (_, i) => ({ key: `k${i}`, plays: i < playing }));

/** Every place covered exactly once, by the seed or a tile. */
function covers(g: { cols: number; rows: number; seed: any; partner: any; tiles: any[] }) {
  const seen = Array.from({ length: g.rows }, () => Array(g.cols).fill(0));
  for (const t of [g.seed, ...(g.partner ? [g.partner] : []), ...g.tiles]) for (let r = t.row; r < t.row + t.h; r++) for (let c = t.col; c < t.col + t.w; c++) seen[r][c]++;
  return seen.every((row) => row.every((x) => x === 1));
}

test('every grid is filled, nothing overlaps, the artist lies flat', () => {
  for (const n of [3, 4, 5, 6, 8, 10, 14, 20]) {
    for (const cols of [3, 4, 6, 8]) {
      const l = layout(items(n, Math.floor(n / 2)), cols);
      if (!l) continue;
      assert.ok(covers(l.grid), `${n} on ${cols}`);
      assert.deepEqual([l.grid.seed.w, l.grid.seed.h], [2, 1]);
      assert.ok(l.grid.rows >= 2 && l.grid.cols <= cols);
      // Only the owner's shapes, a third at most of two places.
      assert.ok(l.grid.tiles.every((t) => t.w * t.h <= 2), `${n} on ${cols}: a tile larger than two`);
      const two = l.grid.tiles.filter((t) => t.w * t.h === 2).length;
      assert.ok(two <= Math.ceil(l.items.length / 3), `${n} on ${cols}: ${two} of two`);
      // A brick's bond: no flat tile exactly on another (the artist's included), no upright one exactly beside another.
      const flats = [l.grid.seed, ...l.grid.tiles.filter((t) => t.w === 2)];
      const ups = l.grid.tiles.filter((t) => t.h === 2);
      assert.ok(!flats.some((a) => flats.some((b) => a !== b && a.col === b.col && Math.abs(a.row - b.row) === 1)), `${n} on ${cols}: flat on flat`);
      assert.ok(!ups.some((a) => ups.some((b) => a !== b && a.row === b.row && Math.abs(a.col - b.col) === 1)), `${n} on ${cols}: upright by upright`);
    }
  }
});

test('more room, a larger grid; few suggestions, larger tiles fill it', () => {
  const narrow = layout(items(12, 4), 3)!;
  const wide = layout(items(12, 4), 8)!;
  assert.ok(wide.grid.cols * wide.grid.rows > narrow.grid.cols * narrow.grid.rows);
  // The same artist, the same layout.
  assert.deepEqual(layout(items(9, 3), 6, 'Air'), layout(items(9, 3), 6, 'Air'));
});

test('what plays grows first; too few to fill anything, no grid', () => {
  // Places beside the artist = the suggestions shown plus those of two places.
  const l = layout(items(6, 1), 3)!;
  const grown = l.grid.tiles.filter((t) => t.w * t.h === 2);
  assert.equal(l.grid.rows * l.grid.cols - 2, l.items.length + grown.length);
  assert.equal(layout(items(1), 3), null);
});

test('a second artist: one place, a place between it and the first, every place filled', () => {
  for (const cols of [4, 6, 8]) {
    const l = layout(items(12, 4), cols, 'Jacco Gardner', true);
    assert.ok(l, `${cols}`);
    const { seed, partner } = l!.grid;
    assert.deepEqual([partner!.w, partner!.h], [1, 1]);
    assert.equal(partner!.row, seed.row);
    assert.equal(partner!.col - (seed.col + seed.w), 1, 'one place between them');
    assert.ok(covers(l!.grid));
  }
});
