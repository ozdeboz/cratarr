// What every menu holds and in what order (decision 0058, amended 2026-10-30; lib/menuTable): the table itself, each
// menu drawn from it (every entry it has a way to show, none it lacks), the names the same everywhere, and the large
// actions in the table's order.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

const { ACTIONS_NEED_SERVER, ENTRY_NAME, ENTRY_WORDS, GROUP_ORDER, ICON_OF, MENUS, OWN_CARD, RETIRED_NAMES, WORKS_AWAY, actionAway, goToName, inOrder, offeredAway, viewName } = await import(
  '../src/lib/menuTable.ts'
);
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
    for (const old of RETIRED_NAMES) {
      // F35.3: a bare View is retired from the menus' entries (a list's View sheet is another thing); RequestsPanel's two sheets
      // keep theirs until RS01 rebuilds them, by name, the one exemption.
      if (old === 'View') {
        if (f !== 'RequestsPanel.svelte') assert.ok(!/<MenuEntry[^>]*\slabel="View"/.test(src), `${f}: still a bare "View"`);
        continue;
      }
      assert.ok(!src.includes(`label="${old}"`), `${f}: still "${old}"`);
    }
  }
});

// F31, F32, F35 and GOTO.1: the navigation words and their icons from the table, read by the components.
test('Go to names the record\'s type, as View and Delete do (GOTO.1, F35.1, F35.4)', () => {
  const types: [string | null | undefined, string[] | undefined, string][] = [
    ['album', undefined, 'album'],
    ['ep', undefined, 'EP'],
    ['single', undefined, 'single'],
    ['broadcast', undefined, 'broadcast'],
    ['other', undefined, 'record'],
    [null, undefined, 'record'],
    [undefined, undefined, 'record'],
    ['album', ['compilation'], 'compilation'],
  ];
  for (const [type, secondary, word] of types) {
    assert.equal(goToName(type, secondary), `Go to ${word}`);
    assert.equal(viewName('album', type, secondary), `View ${word}`);
  }
  // The owner's words, quoted once.
  assert.deepEqual([goToName('ep'), goToName('single'), goToName('album', ['compilation']), goToName(null)], ['Go to EP', 'Go to single', 'Go to compilation', 'Go to record']);
  assert.deepEqual([viewName('album', 'album'), viewName('album', 'ep'), viewName('album', 'single'), viewName('album', 'album', ['compilation']), viewName('album')], ['View album', 'View EP', 'View single', 'View compilation', 'View record']);
  assert.deepEqual([viewName('crate'), viewName('genre'), viewName('playlist')], ['View crate', 'View genre', 'View playlist']);
  assert.equal(ENTRY_NAME.view, undefined, 'View is named by what it is about');
  assert.equal(ENTRY_NAME.goAlbum, undefined, 'Go to is named by what it is about');
  assert.deepEqual([ENTRY_WORDS.saveTo, ENTRY_WORDS.addTo, ENTRY_WORDS.moveTo], ['Save to', 'Add to', 'Move to']);
  const menu = read('AlbumMenu.svelte');
  assert.match(menu, /const word = \$derived\(recordWord\(a\.type, a\.secondary\)\);/, "View's and Delete's one word");
  assert.match(menu, /label=\{viewName\('album', a\.type, a\.secondary\)\}/);
});

test('the navigation words come from the table (F31)', () => {
  const tools = read('HealthPanel.svelte');
  assert.match(tools, /label: goToName\(x\.album_type, x\.album_secondary\)/, "Health's onward names the record's type");
  assert.match(tools, /label: ENTRY_NAME\.goPlaylist!/);
  assert.equal(ENTRY_NAME.goPlaylist, 'Go to playlist');
  assert.match(read('ReplaceTrackSheet.svelte'), /<MenuEntry icon=\{ICON_OF\.outside\} label=\{ENTRY_NAME\.openOnBandcamp!\}/);
  assert.match(read('TrackMenu.svelte'), /label=\{goToName\(t\.album_type, t\.album_secondary\)\}/);
  assert.match(read('SelectionMenu.svelte'), /label=\{goToName\(albumType, albumSecondary\)\}/);
  for (const f of readdirSync(LIB, { recursive: true }).map(String).filter((f) => /\.(svelte|ts)$/.test(f))) {
    const src = read(f.replace(/\\/g, '/'));
    assert.ok(!/(['"`>])Go to album\b/.test(src), `${f}: a literal Go to album`);
    assert.ok(!/Open the album|Open the playlist/.test(src) || f.endsWith('prefs.svelte.ts'), `${f}: Open the album or playlist`);
    assert.ok(!/<MenuEntry[^>]*\slabel="On Bandcamp"/.test(src), f);
  }
});

test('one map of a destination\'s icon (F32), every Go to taking its kind\'s', () => {
  assert.deepEqual(
    [ICON_OF.album, ICON_OF.artist, ICON_OF.playlist, ICON_OF.crate, ICON_OF.genre, ICON_OF.soundsLike, ICON_OF.view, ICON_OF.outside, ICON_OF.requests],
    ['disc', 'user', 'playlists', 'crates', 'swatch', 'music', 'squareArrowOutUpRight', 'externalLink', 'inbox'],
  );
  assert.deepEqual([ICON_OF.saveTo, ICON_OF.addTo, ICON_OF.moveTo], ['save', 'plus', 'intoBox']);
  const icons = read('Icon.svelte');
  for (const name of Object.values(ICON_OF)) assert.match(icons, new RegExp(`\\n\\s+${name}:`), `${name} is one of Icon's`);
  assert.doesNotMatch(read('DownloadsPanel.svelte'), /GO_ICON/);
  for (const f of readdirSync(LIB).filter((f) => f.endsWith('.svelte'))) {
    const src = read(f);
    for (const m of src.matchAll(/<MenuEntry\s[^>]*?label=(?:"Go to [^"]*"|\{goToName\([^}]*\)\}|\{`Go to[^`]*`\})[^>]*>/g)) {
      assert.match(m[0], /icon=\{ICON_OF\.\w+\}|icon=\{ICON_OF\[/, `${f}: ${m[0]}`);
    }
    assert.ok(!/icon="squareArrowOutUpRight"[^>]*label="Go to/.test(src), f);
  }
  assert.match(read('PlaylistView.svelte'), /<Icon name=\{ICON_OF\.album\} size=\{14\} \/>/, "the In your library line's icon");
  assert.match(read('WorkMenu.svelte'), /icon=\{ICON_OF\.work\}\r?\n\s+label="Go to the work"/);
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

// Out of reach (decision 0085; the owner, 2026-10-30): what works without Cratarr shows, what needs it is left out.
test('out of reach: what works offline shows, what needs Cratarr is left out, and in reach everything shows', () => {
  const used = new Set(Object.values(MENUS).flatMap((k) => k.cards.flatMap((c) => c.entries)));
  for (const e of WORKS_AWAY) assert.ok(used.has(e), `${e} is in no menu`);
  for (const e of used) assert.equal(offeredAway(e, false), true, `${e} in reach`);
  // Kept on the device and sent later, the device's own, or another site's.
  for (const e of ['favourite', 'rating', 'excludeEverywhere', 'excludeArtist', 'notInCrate', 'addToPlaylist', 'rename', 'removeFromDevice', 'queues', 'clearQueue', 'webSearch'] as const)
    assert.equal(offeredAway(e, true), true, e);
  // Cratarr's: it streams, finds, makes, deletes or shares them.
  for (const e of ['soundsLike', 'otherVersions', 'discover', 'download', 'keep', 'share', 'info', 'saveAsPlaylist', 'startCrate', 'edit', 'deletePlaylist', 'deleteCrate', 'deleteAlbum', 'sync', 'playFrom'] as const)
    assert.equal(offeredAway(e, true), false, e);
  // A track's menu out of reach keeps its feelings and its playlists, not its Go to Sounds like.
  const track = MENUS.track.cards.flatMap((c) => c.entries).filter((e) => offeredAway(e, true));
  assert.deepEqual(track, ['goAlbum', 'goArtist', 'goGenre', 'favourite', 'excludeEverywhere', 'excludeArtist', 'notInCrate', 'addToPlaylist', 'webSearch', 'geniusSearch', 'removeFromQueue', 'removeFromList']);
});

test('out of reach the large actions that need Cratarr go; playing and queueing stay (what is on this device)', () => {
  for (const a of ACTIONS_NEED_SERVER) {
    assert.equal(actionAway(a, true), false, a);
    assert.equal(actionAway(a, false), true, a);
  }
  for (const a of ['Play', 'Shuffle', 'Play next', 'Add to queue', 'Resume']) assert.equal(actionAway(a, true), true, a);
});
