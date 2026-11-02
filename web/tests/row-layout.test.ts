// One row's layout in one place (lib/rowLayout; the owner, 2026-11-02): what shows and how wide, by rank, for every
// piece of the row at once.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { layoutRow, RETURN, type Piece } from '../src/lib/rowLayout.ts';

const ids = (d: { shown: Set<string> }) => [...d.shown].sort().join(',');
// Back and Up (never go), Settings › Device ▾ › Appearance: a group goes first, then the place, the current one last.
const crumbs: Piece[] = [
  { id: 'nav', width: 96, rank: 0 },
  { id: 'settings', width: 90, rank: 21 },
  { id: 'device', width: 80, rank: 31 },
  { id: 'here', width: 120, rank: 10, min: 48 },
];

test('everything that fits shows, whole', () => {
  const d = layoutRow(400, crumbs);
  assert.equal(ids(d), 'device,here,nav,settings');
  assert.equal(d.width.size, 0);
});

test('short of room: the group first, then the place, then the current level cut, then gone', () => {
  assert.equal(ids(layoutRow(320, crumbs)), 'here,nav,settings', 'the group goes first');
  assert.equal(ids(layoutRow(230, crumbs)), 'here,nav', 'then the place');
  const cut = layoutRow(180, crumbs);
  assert.equal(ids(cut), 'here,nav');
  assert.equal(cut.width.get('here'), 84, 'then the current one cut with …');
  assert.equal(ids(layoutRow(120, crumbs)), 'nav', 'past its least, gone (the title gives way)');
});

test('the open field never narrower than its least; what is left grows it', () => {
  const row: Piece[] = [
    { id: 'pill', width: 100, rank: 0 },
    { id: 'field', width: 140, rank: 0, min: 140, max: 320 },
    ...crumbs,
  ];
  const d = layoutRow(500, row);
  assert.equal(ids(d), 'field,here,nav,pill', 'the levels go before the field is squeezed');
  assert.equal(d.width.get('field'), 184, 'the rest of the room to the field');
  const wide = layoutRow(1000, row);
  assert.equal(wide.width.get('field'), 320, 'at most its most');
});

test('the glass takes Radio first, Play last', () => {
  const row: Piece[] = [
    { id: 'leave', width: 96, rank: 0 },
    { id: 'play', width: 44, rank: 42 },
    { id: 'shuffle', width: 44, rank: 41 },
    { id: 'radio', width: 44, rank: 40 },
    { id: 'pill', width: 150, rank: 0 },
  ];
  assert.equal(ids(layoutRow(400, row)), 'leave,pill,play,radio,shuffle');
  assert.equal(ids(layoutRow(300, row)), 'leave,pill,radio');
  assert.equal(ids(layoutRow(250, row)), 'leave,pill');
});

test('what went comes back only with room to spare, so the row never flickers at the edge', () => {
  const before = layoutRow(320, crumbs).shown;
  assert.ok(!before.has('device'));
  const just = 96 + 90 + 80 + 120;
  assert.ok(!layoutRow(just, crumbs, before).shown.has('device'), 'exactly enough: stays gone');
  assert.ok(layoutRow(just + RETURN, crumbs, before).shown.has('device'), 'with room to spare: back');
});

test('the open field gives way first, to its least, before the title is cut', () => {
  const row: Piece[] = [
    { id: 'title', width: 90, rank: 10, min: 48 },
    { id: 'field', width: 144, rank: 0, min: 96, max: 320 },
  ];
  const d = layoutRow(200, row);
  assert.equal(ids(d), 'field,title');
  assert.equal(d.width.get('field'), 110, 'the field cut');
  assert.equal(d.width.has('title'), false, 'the title whole');
  const tight = layoutRow(160, row);
  assert.equal(tight.width.get('field'), 96, 'the field at its least');
  assert.equal(tight.width.get('title'), 64, 'only then the title cut');
});
