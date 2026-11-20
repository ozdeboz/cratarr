// One loading rule (0.13; the owner, 2026-10-31: "Keep, then spinner at 300 ms"): what was shown stays while its
// replacement is on its way, the loader takes its place at LOADER_AFTER_MS, and the answer replaces either at any time.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { LOADER_AFTER_MS, LOADER_LINE, LOADER_LARGE, LOADER_SMALL, loaderFit, waitShows } = await import('../src/lib/loader.ts');

test('one threshold: 300 ms', () => {
  assert.equal(LOADER_AFTER_MS, 300);
});

test('before the threshold what was shown is kept, never blanked', () => {
  assert.equal(waitShows(1000, 1000), 'kept');
  assert.equal(waitShows(1000, 1000 + LOADER_AFTER_MS - 1), 'kept');
});

test('at and after the threshold the loader takes its place', () => {
  assert.equal(waitShows(1000, 1000 + LOADER_AFTER_MS), 'loader');
  assert.equal(waitShows(1000, 1000 + 10 * LOADER_AFTER_MS), 'loader');
});

test('the answer replaces either, at any time', () => {
  // Nothing waited for: what is there is the answer, however long the wait was.
  for (const now of [0, 100, LOADER_AFTER_MS, 60_000]) assert.equal(waitShows(null, now), 'answer');
});

test('a new wait (the words changed) counts again from its own start', () => {
  const first = 1000;
  const again = first + LOADER_AFTER_MS - 50; // typed on before the loader was due
  assert.equal(waitShows(again, first + LOADER_AFTER_MS + 10), 'kept');
  assert.equal(waitShows(again, again + LOADER_AFTER_MS), 'loader');
});

// The review of 1.2, low 12 (red first: the fit before it was `max(bottom - at, mark * 2)` and the mark's middle alone): a page's
// loader counts the line under its mark in its height, so the words never hang past its box, on a phone's small room too.
test("a loader's line always ends inside its box, whatever room it has", () => {
  for (const mark of [LOADER_SMALL, LOADER_LARGE])
    for (const under of [0, LOADER_LINE, 60])
      for (const [top, bottom, at] of [
        [0, 100, 0],
        [0, 200, 0],
        [56, 640, 56],
        [56, 640, 400],
        [56, 640, 620],
        [0, 300, 200],
      ]) {
        const f = loaderFit({ top, bottom, at, mark, under });
        const room = Math.max(LOADER_LINE, under);
        assert.ok(f.top >= 0, `${mark}/${under}/${at}: never above its box`);
        assert.ok(f.top + mark + room <= f.height, `${mark}/${under}/${top}-${bottom} at ${at}: the line ends at ${f.top + mark + room}, the box at ${f.height}`);
      }
});

test("a small loader in a phone's short room grows by its line, where before its words hung past", () => {
  // A panel's loader (40 px) with 100 px left to the bottom: the mark in the middle left the line 10 px outside.
  assert.deepEqual(loaderFit({ top: 0, bottom: 100, at: 0, mark: LOADER_SMALL, under: 0 }), { height: 120, top: 30 });
});

test('with room enough the mark stays in the middle of its room, as before', () => {
  // A page's loader from the top of an 800 px window: the mark's middle at the room's middle.
  assert.deepEqual(loaderFit({ top: 0, bottom: 800, at: 0, mark: LOADER_LARGE, under: LOADER_LINE }), { height: 800, top: 352 });
  // Below what comes before it, the middle of what is left.
  assert.deepEqual(loaderFit({ top: 56, bottom: 656, at: 256, mark: LOADER_LARGE, under: 0 }), { height: 400, top: 152 });
});
