// One head for every table (lib/columnHead; the owner, 2026-10-30: "Table header rows are inconsistent"): a name the
// list orders by orders it on a tap, again the other way round, starting most first where its order says so; its arrow
// on the name that orders it; a name it cannot order by is plain.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { headCells, headSort, tapped } = await import('../src/lib/columnHead.ts');

const fields = [{ value: 'title' }, { value: 'year' }, { value: 'plays', most: true as const }];
const cols = [
  { id: 'year', label: 'Year', width: 48, end: true },
  { id: 'plays', label: 'Plays', width: 52, end: true },
  { id: 'format', label: 'Format', width: 120 },
];

test('a tap orders by it, as its order starts; again, turned round', () => {
  assert.deepEqual(tapped({ key: 'title', down: false }, 'year', fields), { key: 'year', down: false });
  assert.deepEqual(tapped({ key: 'title', down: false }, 'plays', fields), { key: 'plays', down: true }, 'most first');
  assert.deepEqual(tapped({ key: 'plays', down: true }, 'plays', fields), { key: 'plays', down: false });
});

test('the names it orders by are buttons with their state and arrow; the rest plain', () => {
  let set: unknown = null;
  const cells = headCells(cols, (id: string) => (id === 'format' ? undefined : id), { key: 'plays', down: true }, fields, (o: unknown) => (set = o));
  assert.deepEqual(
    cells.map((c: { id: string; on?: boolean; down?: boolean; onclick?: unknown }) => [c.id, c.on ?? null, c.down ?? null, !!c.onclick]),
    [
      ['year', false, null, true],
      ['plays', true, true, true],
      ['format', null, null, false],
    ],
  );
  cells[1].onclick();
  assert.deepEqual(set, { key: 'plays', down: false });
  assert.equal(cells[0].text, 'Year');
});

test('a column whose order the list has not is plain too', () => {
  const cells = headCells(cols, (id: string) => id, { key: 'title', down: false }, [{ value: 'title' }], () => {});
  assert.ok(cells.every((c: { onclick?: unknown }) => !c.onclick));
});

test('the title or count as an order, where the list has it', () => {
  const s = headSort('title', { key: 'title', down: true }, fields, () => {});
  assert.deepEqual([s?.on, s?.down], [true, true]);
  assert.equal(headSort('name', { key: 'title', down: false }, fields, () => {}), undefined);
  assert.equal(headSort(undefined, { key: 'title', down: false }, fields, () => {}), undefined);
});
