// The snapshot library (library.ts; the owner, 2026-10-29: "why can I not find them in the snapshots panel? This is
// where snapshots should be"): one list that the Snapshots sheet and the panel's Presets both read: the saved ones
// first, then every example once, under their headings: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { EXAMPLES } = await import('../src/lib/visuals/examples.ts');
const { libraryGroups, libraryOf } = await import('../src/lib/visuals/library.ts');

const saved = [
  { id: 'a1', name: 'Bonobo · 21:04' },
  { id: 'a2', name: 'Default' },
];

test('the saved ones first, their time left off, then each example once', () => {
  const lib = libraryOf(saved);
  assert.deepEqual(
    lib.slice(0, 2).map((e) => [e.id, e.label, e.group]),
    [
      ['a1', 'Bonobo', 'saved'],
      ['a2', 'Default', 'saved'],
    ],
  );
  assert.equal(lib[0].saved, saved[0], 'the snapshot itself kept');
  const examples = lib.filter((e) => e.group === 'examples');
  assert.deepEqual(
    examples.map((e) => e.id),
    EXAMPLES.map((x) => x.id),
  );
  assert.equal(new Set(lib.map((e) => e.id)).size, lib.length, 'ids unique');
  assert.ok(examples.some((e) => e.id === 'example:fire'), 'Fire is in it');
});

test('by its groups, each under its heading, an empty one left out', () => {
  assert.deepEqual(
    libraryGroups(libraryOf(saved)).map((g) => [g.label, g.entries.length]),
    [
      ['Saved', 2],
      ['Examples', EXAMPLES.length],
    ],
  );
  assert.deepEqual(
    libraryGroups(libraryOf([])).map((g) => g.label),
    ['Examples'],
  );
});
