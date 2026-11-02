// The tape deck's size in the player's row (lib/tapeDeck deckMost, deckScale): as wide as the row, up to its limit, so
// in a wide panel it stays as it is in a narrow one and never grows over the title or the waveform (the owner,
// 2026-10-31); three times on a 1080p screen in the landscape layout only.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { DECK_H, DECK_W, deckMost, deckScale } = await import('../src/lib/tapeDeck.ts');

test('how far it may grow: its own size on a small screen, half again in portrait, three times in landscape', () => {
  assert.equal(deckMost(false, false), 1);
  assert.equal(deckMost(false, true), 1);
  assert.equal(deckMost(true, false), 1.5);
  assert.equal(deckMost(true, true), 3);
});

test("a panel's row: narrower than the drawing scales it down, a wider one stops at the limit", () => {
  assert.equal(deckScale(DECK_W / 2, 1.5), 0.5);
  assert.equal(deckScale(DECK_W, 1.5), 1);
  // A panel about 500 px wide (its row ~470 px): under the limit, the row's width decides.
  assert.ok(Math.abs(deckScale(470, 1.5) - 470 / DECK_W) < 1e-9);
  // About 1190 px wide: no bigger than at ~480 px, so its height stays that of the narrow panel.
  assert.equal(deckScale(1100, 1.5), 1.5);
  assert.equal(Math.round(DECK_H * deckScale(1100, 1.5)), 65);
  // Landscape's three times still reaches its full size where the row is wide enough.
  assert.equal(deckScale(1100, 3), 3);
  assert.equal(deckScale(0, 3), 0);
});
