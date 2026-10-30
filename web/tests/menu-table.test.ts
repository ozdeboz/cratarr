// What every menu holds and in what order (decision 0058, amended 2026-10-30; lib/menuTable): the table itself, each
// menu drawn from it (every entry it has a way to show, none it lacks), the names the same everywhere, and the large
// actions in the table's order.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

const { ENTRY_NAME, GROUP_ORDER, MENUS, OWN_CARD, RETIRED_NAMES, inOrder } = await import('../src/lib/menuTable.ts');
type Kind = keyof typeof MENUS;

const LIB = new URL('../src/lib/', import.meta.url);
const read = (f: string) => readFileSync(new URL(f, LIB), 'utf8');
/** Each kind's menu. */
const FILES: Record<string, Kind[]> = {
  'TrackMenu.svelte': ['track'],
  'AlbumMenu.svelte': ['album'],
  'ArtistMenu.svelte': ['artist'],
  'PlaylistMenu.svelte': ['playlist'],
  'CrateMenu.svelte': ['crate', 'genre'],
  'QueueMenu.svelte': ['queue'],
  'SelectionMenu.svelte': ['selection'],
  'BandcampAlbumMenu.svelte': ['bandcamp'],
};

test('every kind has its menu', () => {
  assert.deepEqual(Object.values(FILES).flat().sort(), (Object.keys(MENUS) as Kind[]).sort());
});

test("each kind's cards in the groups' order, each entry once, Discover's and the extra on cards of their own", () => {
  for (const [kind, m] of Object.entries(MENUS)) {
    const at = m.cards.map((c) => GROUP_ORDER.indexOf(c.group));
    assert.ok(at.every((i) => i >= 0), `${kind}: a group the order lacks`);
    assert.deepEqual(at, [...at].sort((x, y) => x - y), `${kind}: its groups out of order`);
    const all = m.cards.flatMap((c) => c.entries);
    assert.equal(new Set(all).size, all.length, `${kind}: an entry twice`);
    for (const c of m.cards) {
      if (c.entries.some((e) => OWN_CARD.includes(e))) assert.equal(c.entries.length, 1, `${kind}: ${c.entries[0]} shares its card`);
    }
  }
});

test('0058: Play next and Add to queue are not a playlist\'s, a crate\'s or a genre\'s; Delete is last', () => {
  for (const kind of ['playlist', 'crate', 'genre'] as Kind[]) {
    assert.ok(!MENUS[kind].actions.includes('Play next') && !MENUS[kind].actions.includes('Add to queue'), kind);
  }
  for (const [kind, m] of Object.entries(MENUS)) {
    const all = m.cards.filter((c) => c.group !== 'extra').flatMap((c) => c.entries);
    const del = all.findIndex((e) => e.startsWith('delete'));
    if (del >= 0) assert.equal(del, all.length - 1, `${kind}: Delete not last`);
  }
});

test("Bandcamp's previews are never downloaded, kept or put in a playlist (decision 0071)", () => {
  const all = MENUS.bandcamp.cards.flatMap((c) => c.entries);
  for (const e of ['download', 'keep', 'addToPlaylist'] as const) assert.ok(!all.includes(e), e);
  assert.ok(MENUS.bandcamp.actions.includes('Play next') && MENUS.bandcamp.actions.includes('Add to queue'), 'queued as any track');
});

test('each menu is drawn from the table: every entry of its kinds has a way to show, and it has none other', () => {
  for (const [file, kinds] of Object.entries(FILES)) {
    const src = read(file);
    assert.match(src, /<MenuTable\b/, `${file}: not drawn from the table`);
    const want = new Set(kinds.flatMap((k) => MENUS[k].cards.flatMap((c) => c.entries)));
    const has = new Set([...src.matchAll(/\be === '(\w+)'/g)].map((m) => m[1]));
    for (const e of want) assert.ok(has.has(e), `${file}: no way to show ${e}`);
    for (const e of has) assert.ok(want.has(e as never), `${file}: shows ${e}, which the table does not give it`);
  }
});

test('the names are the same everywhere (Info, View, Sync to this device, Download…), and the old ones are gone', () => {
  for (const [file, kinds] of Object.entries(FILES)) {
    const src = read(file);
    for (const e of new Set(kinds.flatMap((k) => MENUS[k].cards.flatMap((c) => c.entries)))) {
      const name = ENTRY_NAME[e];
      if (name) assert.ok(src.includes(`label="${name}"`), `${file}: ${e} not named "${name}"`);
    }
  }
  for (const f of readdirSync(LIB).filter((f) => f.endsWith('.svelte'))) {
    const src = read(f);
    for (const old of RETIRED_NAMES) assert.ok(!src.includes(`label="${old}"`), `${f}: still "${old}"`);
  }
});

test("the large actions in the table's order, equals as given, one it does not name last", () => {
  const a = (label: string) => ({ label });
  assert.deepEqual(inOrder('album', [a('Add to queue'), a('Play'), a('Shuffle'), a('Resume')]).map((x) => x.label), ['Play', 'Resume', 'Shuffle', 'Add to queue']);
  assert.deepEqual(inOrder('track', [a('Start radio'), a('Odd'), a('Play next')]).map((x) => x.label), ['Play next', 'Start radio', 'Odd']);
  for (const file of Object.keys(FILES)) {
    if (file === 'QueueMenu.svelte') continue;
    assert.match(read(file), /inOrder\(/, `${file}: its large actions not in the table's order`);
  }
});
