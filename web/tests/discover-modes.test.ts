// Discover's modes (lib/discoverModes; the owner, 2026-10-29): each suggestion in the one whose reason fits it best
// (the server's `mode`), worked out from why it was suggested where a server from before gives none, and each mode in
// its own order.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { inModeOrder, modeOf } = await import('../src/lib/discoverModes.ts');
type S = Parameters<typeof modeOf>[0];
const s = (x: Partial<S>): S => ({ why: [], new_artist: false, year: null, ...x });
const NOW = Date.UTC(2026, 9, 29);

test("the server's mode stands", () => {
  assert.equal(modeOf(s({ mode: 'gaps', why: ['New from X'] }), NOW), 'gaps');
});

test('from a server before it: fresh, then gaps, then new artists, then alike', () => {
  assert.equal(modeOf(s({ why: ['New from Bonobo'] }), NOW), 'fresh');
  assert.equal(modeOf(s({ released: '2026-09-01' }), NOW), 'fresh', 'out within three months');
  assert.equal(modeOf(s({ released: '2026-06' }), NOW), 'alike', 'out longer ago');
  assert.equal(modeOf(s({ why: ["Missing from Boards of Canada's albums"] }), NOW), 'gaps');
  assert.equal(modeOf(s({ new_artist: true }), NOW), 'artists');
  assert.equal(modeOf(s({ why: ['Like what you play'] }), NOW), 'alike');
});

test('each mode in its order', () => {
  const fresh = inModeOrder('fresh', [s({ released: '2026-08-01', year: 2026 }), s({ released: '2026-10-01' }), s({ year: 2025 })]);
  assert.deepEqual(fresh.map((x) => x.released ?? x.year), ['2026-10-01', '2026-08-01', 2025]);
  const gaps = inModeOrder('gaps', [s({ artist_plays: 3 }), s({ artist_plays: 40 }), s({})]);
  assert.deepEqual(gaps.map((x) => x.artist_plays ?? 0), [40, 3, 0]);
  const alike = inModeOrder('alike', [s({ score: 0.2, year: 1 }), s({ score: 0.9, year: 2 }), s({ score: 0.2, year: 3 })]);
  assert.deepEqual(alike.map((x) => x.year), [2, 1, 3], 'the best fit first, equals as given');
});
