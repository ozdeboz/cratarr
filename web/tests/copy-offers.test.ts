// What each of a record's copies allows, and how its menu names them (lib/copiesRule, docs/records-and-copies.md, "On the
// page"): an offer per copy, a target already holding a copy is out, Replace from an upgrade into any main library
// holding a copy, and the entries saying which copy where a record has several.
import { test } from 'node:test';
import assert from 'node:assert/strict';

import { readFileSync } from 'node:fs';
import type { AlbumCopy, AlbumsSource, Against, LibraryInfo } from '../src/lib/api.ts';

const { moveSentences, moveWords, withCopyAdded } = await import('../src/lib/copiesRule.ts');
const { afterReplace, replacedWords, replaceSheet } = await import('../src/lib/copiesRule.ts');
const { MENUS, offeredAway } = await import('../src/lib/menuTable.ts');
const { adds, copies, deleteEntries, deleteGroups, heardAs, moveEntries, moveGroups, offersOf, playFromEntries, replaceEntries, withCopyMoved, withoutCopy } = await import('../src/lib/copiesRule.ts');

const library = (id: string, kind: LibraryInfo['kind'], more: Partial<LibraryInfo> = {}) => ({ id, name: id.toUpperCase(), kind, ...more }) as LibraryInfo;
const INFO = [library('music', 'main'), library('more', 'main'), library('in', 'incoming'), library('in2', 'incoming'), library('own', 'own', { mine: true }), library('theirs', 'own')];

const copy = (id: string, more: Partial<AlbumCopy> = {}) => ({ library: id, deletable: true, keepable: false, keep_to: null, ...more }) as AlbumCopy;
const list = (more: Partial<AlbumsSource> = {}) => ({ library_info: INFO, ...more }) as AlbumsSource;
/** An Incoming copy's stand against the record's copy in `library` (C37a). */
const against = (library: string, better = true, replaces = better): { against: Against } => ({ against: { better, replaces, library, dir: 'A/B', why: '' } });
const none = () => null;
const offers = (copies: AlbumCopy[], s = list(), admin = true) => offersOf(s, { id: 'al-1', copies }, admin, none);
const ids = (l: { id: string }[]) => l.map((x) => x.id);

// F33.2 (changed by design, the owner's "Save to", AMENDMENTS 2026-11-19 ~11:25): a copy says Save.
test("a copy keeps the original's copy and says Save", () => {
  const said = moveSentences({ title: 'Two Rooms', from: 'Music', to: "Ann's music", folder: 'A/Two Rooms', files: 12, copy: true, adding: true });
  assert.equal(said.ask, "Save Two Rooms to Ann's music as A/Two Rooms (12 files)? The album stays in Music.");
  assert.equal(said.button, "Save to Ann's music");
  assert.equal(said.done, "Saved Two Rooms to Ann's music.");
  assert.equal(said.undone, "Two Rooms is no longer in Ann's music.");
  assert.deepEqual(moveWords(true, true), { verb: 'Save', none: 'saved to', past: 'Saved', not: 'Not saved', act: 'save' });
  for (const s of [said.ask, said.button, said.done, said.undone]) assert.ok(!/\b(Copy|Copied)\b/.test(s), s);
  assert.ok(!said.undone.includes('back in Music'), said.undone);
  for (const s of [said.ask, said.button, said.done, said.undone]) assert.ok(!/\b(Add|Move|Moved)\b/.test(s), s);
  const record = { library: undefined, format: 'MP3', bitrate_kbps: 320, copies: [copy('music', { plays: true })] };
  const after = withCopyAdded(record, 'music', 'ann');
  assert.deepEqual(after.copies!.map((c) => [c.library, c.plays]), [['music', true], ['ann', false]], "Music's copy as it was, Ann's added");
  // T3.2: own music into Music.
  const back = moveSentences({ title: 'Two Rooms', from: "Ann's music", to: 'Music', folder: 'A/Two Rooms', files: 12, copy: true, adding: false });
  assert.equal(back.ask, "Save Two Rooms to Music as A/Two Rooms (12 files)? The album stays in Ann's music.");
  assert.ok(copies(library('ann', 'own', { owner: 'ann' }), library('music', 'main')), "Ann's own music into Music is a copy");
});

test("a leaver's own music into Music reads Move, as the server moves it", () => {
  const info = [library('music', 'main', { name: 'Music' }), library('gone', 'own', { owner: null, name: "gone's music" })];
  assert.equal(copies(info[1], info[0]), false, "the server moves a departed user's music");
  const said = moveSentences({ title: 'Two Rooms', from: "gone's music", to: 'Music', folder: 'A/Two Rooms', files: 12, copy: false, adding: false });
  assert.ok(said.ask.startsWith('Move Two Rooms into Music') && said.done.startsWith('Moved ') && !said.ask.includes('stays'), said.ask);
  const five = Array.from({ length: 5 }, (_, i) => ({ offers: offersOf(list({ library_info: info }), { id: `al-${i}`, copies: [copy('gone')] }, true, none) }));
  const [group] = moveGroups(five).filter((g) => g.name === 'Music');
  assert.equal(group.copying, false);
  assert.equal(`${moveWords(group.copying, group.adding).verb} 5 albums to ${group.name}?`, 'Move 5 albums to Music?');
  assert.deepEqual(withCopyMoved({ library: undefined, format: 'MP3', bitrate_kbps: 320, copies: [copy('gone', { plays: true })] }, 'gone', 'music').copies!.map((c) => c.library), ['music'], 'its copy leaves that library');
});

test("the record menu and several albums take their words and icons from one place", () => {
  const read = (f: string) => readFileSync(new URL(`../src/lib/${f}`, import.meta.url), 'utf8');
  const [menu, several] = [read('AlbumMenu.svelte'), read('SeveralAlbums.svelte')];
  assert.ok(/moveSentences/.test(menu) && /keep\??\.copy/.test(menu) && /copiesAdded\(/.test(menu), 'the record menu: the preview\'s copy, the shared words, copiesAdded');
  assert.ok(!/'Added' : 'Moved'/.test(menu) && !/adding \? 'Add' : 'Move'/.test(menu), 'no words of its own');
  assert.ok(!/const moveWords/.test(several) && /moveWords\(/.test(several) && /copiesAdded\(/.test(several), 'several albums: the shared words, copiesAdded');
  // F33.1, F33.3: the entries' words from the table, their icons from the one map; neither builds its own.
  assert.match(menu, /<MenuEntry icon=\{ICON_OF\[`\$\{m\.verb\}To`\]\} label=\{m\.label\}/);
  assert.match(several, /icon=\{ICON_OF\[`\$\{verb\}To`\]\}\r?\n      label=\{moveLabel\(verb, mv\.name\)\}/);
  assert.ok(!/'Copy' : mv\.adding \? 'Add' : 'Move'/.test(several) && !/icon="intoBox"/.test(several + menu), 'no words or icon of their own');
});

test('a record has one offer per copy, in the order the list gives them', () => {
  assert.deepEqual(offers([copy('own'), copy('music')]).map((o) => o.library), ['own', 'music']);
});

test('two folders of one library are one offer, as `from` names the first', () => {
  assert.deepEqual(offers([copy('music'), copy('music', { deletable: false })]).map((o) => [o.library, o.deletable]), [['music', true]]);
});

test("a copy not its viewer's to delete offers no Delete, while the copy beside it does", () => {
  assert.deepEqual(offers([copy('music', { deletable: false }), copy('in')]).map((o) => o.deletable), [false, true]);
});

test('a target already holding a copy is out: a record in Music and More cannot move between them', () => {
  const [music, more] = offers([copy('music'), copy('more')]);
  assert.deepEqual(music.targets.map((l) => l.id).filter((id) => id === 'more'), []);
  assert.deepEqual(more.targets.map((l) => l.id).filter((id) => id === 'music'), []);
});

// F34.1 (changed by design, the owner, AMENDMENTS 2026-11-19 ~11:30): an Incoming library is filled by its fetcher alone.
test('a kept copy goes into any other main library, or own music as a copy', () => {
  assert.deepEqual(ids(offers([copy('music')])[0].targets), ['more', 'own', 'theirs']);
});

test('F34.2: with one main library, a main album has no Move to', () => {
  const s = list({ library_info: [library('music', 'main'), library('in', 'incoming'), library('in2', 'incoming')] });
  assert.deepEqual(moveEntries(offers([copy('music')], s)), []);
});

test("F34.3: Incoming's Add to and its Undo as before; several kept records never name an Incoming library", () => {
  assert.deepEqual(ids(offers([copy('in', { keepable: true, keep_to: 'more' })])[0].targets), ['more', 'music', 'own']);
  assert.equal(moveSentences({ title: 'Two Rooms', from: 'IN', to: 'MORE', folder: 'A/Two Rooms', files: 12, copy: false, adding: true }).undone, 'Two Rooms is back in IN.');
  const groups = moveGroups([ticked('al-1', copy('music')), ticked('al-2', copy('more'))]);
  assert.ok(!groups.some((g) => g.name === 'IN' || g.name === 'IN2'), groups.map((g) => g.name).join(', '));
});

test('only an administrator moves or copies a kept copy, out of own music as a copy', () => {
  assert.deepEqual(offers([copy('music')], list(), false)[0].targets, []);
  assert.deepEqual(ids(offers([copy('own')])[0].targets), ['music', 'more', 'theirs'], 'never into Incoming');
});

test("into or out of someone's own music a kept record is copied: Save to, alone or with others", () => {
  // F33.1, F34.1 (changed by design): Save to for a copy, its icon by its verb; no Incoming target for a kept copy.
  assert.deepEqual(moveEntries(offers([copy('music')])).map((e) => [e.label, e.verb]), [['Move to MORE', 'move'], ['Save to OWN', 'save'], ['Save to THEIRS', 'save']]);
  assert.deepEqual(moveEntries(offers([copy('own')])).map((e) => e.label), ['Save to MUSIC', 'Save to MORE', 'Save to THEIRS']);
  assert.deepEqual(moveEntries(offers([copy('in', { keepable: true, keep_to: 'more' })])).map((e) => [e.label, e.verb]), [['Add to MORE', 'add'], ['Add to MUSIC', 'add'], ['Add to OWN', 'add']]);
  assert.equal(copies(INFO[0], INFO[4]), true, 'Music into own music');
  assert.equal(copies(INFO[4], INFO[0]), true, 'own music into Music');
  assert.equal(copies(INFO[2], INFO[4]), false, 'out of Incoming: added');
  assert.equal(copies(INFO[0], INFO[1]), false, 'between main libraries: moved');
  const groups = moveGroups([ticked('al-1', copy('music')), ticked('al-2', copy('more'))]);
  assert.deepEqual(groups.filter((g) => g.copying).map((g) => g.name), ['OWN', 'THEIRS']);
});

test("an Incoming copy is added where it goes by default first, then an administrator's other choices", () => {
  const [o] = offers([copy('in', { keepable: true, keep_to: 'more' })]);
  assert.deepEqual(ids(o.targets), ['more', 'music', 'own']);
});

test('a user adds an Incoming copy only where it goes by default', () => {
  const [o] = offers([copy('in', { keepable: true, keep_to: 'own' })], list(), false);
  assert.deepEqual(ids(o.targets), ['own']);
});

test("an Incoming copy not the viewer's to add has no targets", () => {
  assert.deepEqual(offers([copy('in', { keep_to: 'music' })])[0].targets, []);
});

test("a better copy replaces the record's copy in any main library; one no better, or better than own music's alone, none", () => {
  const [incoming, music, more] = offers([copy('in', against('more')), copy('music'), copy('more')]);
  assert.deepEqual(ids(incoming.replaceInto), ['music', 'more']);
  assert.deepEqual([music.replaceInto, more.replaceInto], [[], []], 'a kept copy replaces nothing');
  assert.deepEqual(offers([copy('in', against('music', false)), copy('music')])[0].replaceInto, [], 'no better');
  assert.deepEqual(offers([copy('in', against('own', true, false)), copy('own'), copy('music')])[0].replaceInto, [], "better than someone's own copy alone");
  assert.deepEqual(offers([copy('in'), copy('music')])[0].replaceInto, [], 'no stand');
});

test('an Incoming copy may not replace what it may not delete', () => {
  assert.deepEqual(offers([copy('in', { deletable: false, ...against('music') }), copy('music')])[0].replaceInto, []);
});

// RY02 (SCENARIOS-RUN-12): Replace in a main library from someone's own copy (RY01), offered to administrators, the line naming
// whose copy it is (the owner, 2026-11-17): "Replace in Music from Your music", "Replace in Music from <name>'s music".
const INFO2 = [
  library('music', 'main', { name: 'Music' }),
  library('own', 'own', { mine: true, name: 'Your music', owner: 'root' }),
  library('bob', 'own', { name: "Bob's music", owner: 'bob' }),
  library('ann', 'own', { name: "Ann's music", owner: 'ann' }),
  library('more', 'main', { name: 'More' }),
];
/** Someone's own copy's stands as RY01 lists them (`replace_into`): it beats the copy in each of `libraries`. */
const replacing = (...libraries: string[]): { replace_into: Against[] } => ({ replace_into: libraries.map((library) => ({ better: true, replaces: true, library, dir: 'A/B', why: '' })) });
const s2 = list({ library_info: INFO2 });

test("an administrator replaces Music's copy from the better own copy, named by whose music it is", () => {
  const a = replaceEntries(offersOf(s2, { id: 'al-1', copies: [copy('music'), copy('bob', { deletable: false, ...replacing('music') })] }, true, none));
  assert.deepEqual(a.map((e) => [e.label, e.offer.library, e.to.id]), [["Replace in Music from Bob's music", 'bob', 'music']]);
  const b = replaceEntries(offersOf(s2, { id: 'al-2', copies: [copy('music'), copy('own', replacing('music'))] }, true, none));
  assert.deepEqual(b.map((e) => e.label), ['Replace in Music from Your music']);
  // Never "from Your music" for Bob's copy, nor an entry on Music's own copy.
  assert.ok(!a.some((e) => e.label.includes('Your music')), "Bob's copy is Bob's");
  assert.ok(![...a, ...b].some((e) => e.offer.library === 'music'), "Music's copy replaces nothing");
  // OQ-M: an own library whose account is gone goes by its name as the server gives it, as every other menu does.
  const gone = list({ library_info: [...INFO2, library('gone', 'own', { owner: null, name: "gone's music" })] });
  assert.deepEqual(replaceEntries(offersOf(gone, { id: 'al-3', copies: [copy('music'), copy('gone', replacing('music'))] }, true, none)).map((e) => e.label), ["Replace in Music from gone's music"]);
});

test('no Replace from own music for a user, for a copy no better, nor one with a job; of two better, the best alone', () => {
  const lines = (copies: AlbumCopy[], admin = true, jobOf: (library?: string) => number | null = none, s = s2) => replaceEntries(offersOf(s, { id: 'al-1', copies }, admin, jobOf)).map((e) => e.label);
  // 1. A user, Bob himself included.
  assert.deepEqual(lines([copy('music'), copy('bob', replacing('music'))], false), [], 'a user');
  const bobs = list({ library_info: INFO2.map((l) => (l.id === 'bob' ? { ...l, mine: true, name: 'Your music' } : l.id === 'own' ? { ...l, mine: false, name: "root's music" } : l)) });
  assert.deepEqual(lines([copy('music'), copy('bob', replacing('music'))], false, none, bobs), [], 'Bob himself');
  // 2. No stand, or one that does not replace.
  assert.deepEqual(lines([copy('music'), copy('bob')]), [], 'no stand');
  const stand = (better: boolean, replaces: boolean) => ({ replace_into: [{ better, replaces, library: 'music', dir: 'A/B', why: '' }] });
  assert.deepEqual(lines([copy('music'), copy('bob', stand(false, false))]), [], 'no better');
  assert.deepEqual(lines([copy('music'), copy('bob', stand(true, false))]), [], 'the server did not say it replaces');
  // 3. A conversion at work on Bob's copy.
  assert.deepEqual(lines([copy('music'), copy('bob', replacing('music'))], true, (l) => (l === 'bob' ? 4 : null)), [], 'a job');
  // 4. Two own copies better than Music's, the server's stand on the best one only (Ann's 24/96).
  assert.deepEqual(lines([copy('music'), copy('bob'), copy('ann', replacing('music'))]), ["Replace in Music from Ann's music"]);
  // OQ-N: one line per main library the own copy beats, none for one it does not beat; never two for one library.
  assert.deepEqual(lines([copy('music'), copy('more'), copy('bob', replacing('music', 'more'))]), ["Replace in Music from Bob's music", "Replace in More from Bob's music"]);
  assert.deepEqual(lines([copy('music'), copy('more'), copy('bob', replacing('music'))]), ["Replace in Music from Bob's music"], 'not More, which it does not beat');
  assert.deepEqual(lines([copy('music'), copy('music'), copy('bob', replacing('music'))]), ["Replace in Music from Bob's music"], 'two folders of Music, one line');
});

test("Replace from someone's own music is offered though the copy is not the administrator's to delete", () => {
  const o = offersOf(s2, { id: 'al-1', copies: [copy('music'), copy('bob', { deletable: false, keepable: false, ...replacing('music') })] }, true, none);
  const bob = o.find((x) => x.library === 'bob')!;
  assert.deepEqual(ids(bob.replaceInto), ['music'], 'from its stand');
  assert.deepEqual(bob.targets, [], 'no Move');
  assert.deepEqual(moveEntries(o).filter((e) => e.offer.library === 'bob'), [], 'no Move');
  assert.deepEqual(deleteEntries(o, 'album').filter((d) => d.offer.library === 'bob'), [], 'no Delete');
});

test("a replace from own music leaves the own copy on the record, and its undo names Music's copy", () => {
  const a = { id: 'al-1', library: 'music', format: 'MP3', bitrate_kbps: 320, copies: [copy('music', { plays: true }), copy('bob')] };
  const after = afterReplace(a, 'bob', 'own');
  assert.deepEqual(after!.copies!.map((c) => c.library), ['music', 'bob'], 'Bob keeps his');
  assert.deepEqual([after!.library, after!.format, after!.bitrate_kbps], ['music', 'MP3', 320], "Music's copy still listed, its format with the next read");
  const said = replacedWords('Both Shores', 'Music', "Bob's music", 'own');
  assert.equal(said.done, 'Replaced Both Shores in Music.');
  assert.equal(said.undone, "Music's copy of Both Shores is back.");
  for (const s of [said.done, said.undone]) assert.ok(!s.includes("Bob's music") && !/\bmoved?\b/i.test(s), s);
  // From Incoming, as ever: its copy took Music's place and leaves the record, and the Undo puts it back in Incoming.
  const waiting = { ...a, copies: [copy('music', { plays: true }), copy('in')] };
  assert.deepEqual(afterReplace(waiting, 'in', 'incoming')!.copies!.map((c) => c.library), ['music']);
  assert.equal(replacedWords('Both Shores', 'Music', 'IN', 'incoming').undone, 'Both Shores is back in IN.');
});

test('no Replace from own music offline or on a list that names no copies', () => {
  // The device's own list: no copies, no libraries.
  assert.deepEqual(replaceEntries(offersOf(list({ library_info: [] }), { id: 'al-1' }, true, none)), [], "the device's list");
  // Another server's list in its mode: its one library, no copies.
  assert.deepEqual(replaceEntries(offersOf(s2, { id: 'al-1', library: 'bob' }, true, none)), [], "another server's list");
  // Cratarr out of reach: the list as kept still says Bob's copy beats Music's, and the record's menu leaves Replace out, as its
  // tap needs Cratarr.
  assert.equal(replaceEntries(offersOf(s2, { id: 'al-1', copies: [copy('music'), copy('bob', replacing('music'))] }, true, none)).length, 1, 'the list as kept');
  const entries = MENUS.album.cards.flatMap((c) => c.entries);
  assert.ok(entries.includes('replaceIn') && !entries.filter((e) => offeredAway(e, true)).includes('replaceIn'), 'out of reach: no Replace');
});

test('the record menu takes its Replace words from copiesRule', () => {
  const read = (f: string) => readFileSync(new URL(`../src/lib/${f}`, import.meta.url), 'utf8');
  const [menu, albums] = [read('AlbumMenu.svelte'), read('albums.svelte.ts')];
  assert.match(menu, /replaceEntries\(copyOffers\)/, 'its entries');
  assert.match(
    menu,
    /const said = replacedWords\(r\.title, into\.name, fromName, kind\);\s*moved\(r, said\.done, \(\) => copiesReplaced\(\[\{ source, id: a\.id, from, kind \}\]\), from, said\.undone\);/,
    'its left step and its Undo words',
  );
  assert.match(albums, /changeListed\(replaced, \(a, i\) => afterReplace\(a, replaced\[i\]\.from, replaced\[i\]\.kind\)\)/, "the left step is the rule's");
  assert.doesNotMatch(menu, /`Replaced \$\{|copiesGone\(\[\{ source, id: a\.id, from \}\]\)/, 'no words or left step of its own');
  // Its sheet: from someone's own music no cover choice, no carrying over, nothing of Lidarr or Incoming (OQ-A), and the tap says
  // what the sheet showed (OQ-G).
  assert.deepEqual(replaceSheet('own'), { cover: false, carry: false, lidarr: false, shown: true });
  assert.deepEqual(replaceSheet('incoming'), { cover: true, carry: true, lidarr: true, shown: false });
  assert.match(menu, /const swapAsks = \$derived\(replaceSheet\(copy\?\.from\?\.kind\)\);/);
  const sheet = menu.slice(menu.indexOf("{:else if ask === 'replace'}"), menu.indexOf("{:else if ask === 'cover-url'}"));
  assert.match(sheet, /\{#if swapAsks\.cover && swap\.covers && /, 'the cover choice');
  assert.equal(sheet.split('swap.carry?.length').length, sheet.split('swapAsks.carry && swap.carry?.length').length, 'carrying over, every time');
  assert.equal(sheet.split('NO_LIDARR').length, 2, 'one Lidarr line');
  assert.match(sheet, /\{#if !swap\.acquisition && swapAsks\.lidarr\}\s*\{` \$\{NO_LIDARR\}`\}\{\/if\}/, 'said only as a copy leaves Incoming');
  assert.doesNotMatch(sheet, /Incoming/);
  // RF10: what it showed, by the one rule, the own copy's folder with it.
  assert.match(menu, /const shown = asks\.shown \? replaceShown\(swap\) : undefined;/);
  assert.match(menu, /replaceIncoming\(source, a\.id, from, into\.id, year\.trim\(\) \|\| undefined, asks\.cover && swap\?\.covers \? coverWay : undefined, asks\.carry && carry, shown\)/);
});

test("another server's album comes with its one copy, and goes by what that copy allows", () => {
  const [o, ...rest] = offers([copy('in', { keepable: true, keep_to: 'more' })]);
  assert.equal(rest.length, 0);
  assert.deepEqual([o.library, o.deletable, ids(o.targets)[0]], ['in', true, 'more']);
});

test('a copy its viewer may not delete goes nowhere', () => {
  assert.deepEqual(offers([copy('music', { deletable: false })])[0].targets, []);
});

test('a list with no libraries names no copy and allows nothing', () => {
  const [o] = offersOf(list(), { id: 'al-1' }, true, none);
  assert.deepEqual([o.library, o.deletable, o.targets], [undefined, false, []]);
});

test('a record no one has has nothing to offer', () => {
  assert.deepEqual(offers([]), []);
});

test("each offer knows its own copy's job", () => {
  const o = offersOf(list(), { id: 'al-1', copies: [copy('in'), copy('music')] }, true, (l) => (l === 'in' ? 7 : null));
  assert.deepEqual(o.map((x) => x.job), [7, null]);
});

test('a copy with a job has no Move, Replace or Delete, and the copy beside it keeps them', () => {
  const o = offersOf(list(), { id: 'al-1', copies: [copy('in', { keepable: true, ...against('music') }), copy('music')] }, true, (l) => (l === 'in' ? 7 : null));
  assert.deepEqual([...new Set(moveEntries(o).map((e) => e.offer.library))], ['music']);
  assert.deepEqual(replaceEntries(o), []);
  assert.deepEqual(deleteEntries(o, 'album').map((e) => e.label), ['Delete from MUSIC']);
});

test('one copy says Delete by its type, several each name their library', () => {
  assert.deepEqual(deleteEntries(offers([copy('music')]), 'EP').map((e) => e.label), ['Delete EP']);
  assert.deepEqual(deleteEntries(offers([copy('music'), copy('own')]), 'EP').map((e) => e.label), ['Delete from MUSIC', 'Delete from OWN']);
});

test('of several copies, the one the viewer may delete is still named', () => {
  assert.deepEqual(deleteEntries(offers([copy('music', { deletable: false }), copy('own')]), 'EP').map((e) => e.label), ['Delete from OWN']);
});

test('the name is the server\'s, so "Your music" comes for free', () => {
  const s = list({ library_info: [...INFO.slice(0, 4), library('own', 'own', { name: 'Your music', mine: true })] });
  assert.deepEqual(deleteEntries(offers([copy('music'), copy('own')], s), 'EP').map((e) => e.label), ['Delete from MUSIC', 'Delete from Your music']);
});

test('Add to, Move to or Save to as the copy goes, each entry knowing its copy and its target', () => {
  const e = moveEntries(offers([copy('in', { keepable: true, keep_to: 'own' }), copy('music')]));
  assert.deepEqual(e.map((x) => [x.label, x.offer.library, x.to.id]), [
    ['Add to OWN from IN', 'in', 'own'],
    ['Add to MORE from IN', 'in', 'more'],
    ['Move to MORE from MUSIC', 'music', 'more'],
    ['Save to OWN from MUSIC', 'music', 'own'],
    ['Save to THEIRS from MUSIC', 'music', 'theirs'],
  ]);
});

// The owner, testing 0.16.0: on Drift, Music's MP3 and Your music's FLAC, "Move to lidarr-music" moved Music's copy, where the FLAC
// was meant. Every action on a record of several copies names the copy it acts on, not only where two would read the same.
test('a record of several copies names the copy each Move and Replace acts on; one copy names none', () => {
  const s = list({ library_info: [...INFO, library('lidarr', 'main', { name: 'lidarr-music' })] });
  assert.deepEqual(moveEntries(offers([copy('music'), copy('own')], s)).filter((x) => x.to.id === 'lidarr').map((x) => x.label), ['Move to lidarr-music from MUSIC', 'Save to lidarr-music from OWN'], "own music's copy goes as a copy (C40)");
  assert.deepEqual(replaceEntries(offers([copy('in', against('music')), copy('music'), copy('more')])).map((x) => x.label), ['Replace in MUSIC from IN', 'Replace in MORE from IN']);
  assert.deepEqual(moveEntries(offers([copy('music')], s)).filter((x) => x.to.id === 'lidarr').map((x) => x.label), ['Move to lidarr-music']);
});

test('two copies that would say the same each name themselves, and their keys differ', () => {
  const e = moveEntries(offers([copy('music'), copy('more')], list({ library_info: [...INFO, library('third', 'main')] })));
  assert.deepEqual(e.filter((x) => x.to.id === 'third').map((x) => x.label), ['Move to THIRD from MUSIC', 'Move to THIRD from MORE']);
  assert.equal(new Set(e.map((x) => x.key)).size, e.length, 'a keyed each throws on a repeat');
});

test('Replace has an entry per main library, named by it', () => {
  const o = offers([copy('in', against('music')), copy('music'), copy('more')]);
  assert.deepEqual(replaceEntries(o).map((e) => [e.label, e.offer.library, e.to.id]), [
    ['Replace in MUSIC from IN', 'in', 'music'],
    ['Replace in MORE from IN', 'in', 'more'],
  ]);
});

test('out of Incoming or into own music is an add, between main libraries a move', () => {
  assert.equal(adds(INFO[2], INFO[0]), true);
  assert.equal(adds(INFO[0], INFO[4]), true);
  assert.equal(adds(INFO[0], INFO[1]), false);
});

// Several ticked records (Q17: one entry per library, as `moves` groups; no Play from).
const ticked = (id: string, ...copies: AlbumCopy[]) => ({ id, offers: offersOf(list(), { id, copies }, true, none) });

test('ticked records of one copy each keep one Delete, for all of them', () => {
  const groups = deleteGroups([ticked('al-1', copy('music')), ticked('al-2', copy('own'))]);
  assert.deepEqual(groups.map((g) => [g.name, g.them.map((x) => x.offer.library)]), [[null, ['music', 'own']]]);
});

test('with a record of two copies among them, Delete is an entry per library', () => {
  const groups = deleteGroups([ticked('al-1', copy('music')), ticked('al-2', copy('music'), copy('own'))]);
  assert.deepEqual(groups.map((g) => [g.name, g.them.map((x) => x.item.id)]), [
    ['MUSIC', ['al-1', 'al-2']],
    ['OWN', ['al-2']],
  ]);
});

test('a copy with a job or not deletable is in no Delete, and with nothing to delete there is none', () => {
  const jobbed = { id: 'al-1', offers: offersOf(list(), { id: 'al-1', copies: [copy('music')] }, true, () => 7) };
  assert.deepEqual(deleteGroups([jobbed, ticked('al-2', copy('own', { deletable: false }))]), []);
});

test('a record moves once into a library, by the first of its copies that can', () => {
  const [more] = moveGroups([ticked('al-1', copy('music'), copy('in', { keepable: true }))]).filter((g) => g.name === 'MORE');
  assert.deepEqual(more.them.map((x) => [x.item.id, x.offer.library]), [['al-1', 'music']]);
});

test('records move by library name, each its own copy, added only where every one is', () => {
  const [more] = moveGroups([ticked('al-1', copy('music')), ticked('al-2', copy('in', { keepable: true }))]).filter((g) => g.name === 'MORE');
  assert.deepEqual(more.them.map((x) => [x.offer.library, x.add]), [['music', false], ['in', true]]);
  assert.equal(more.adding, false);
  assert.equal(moveGroups([ticked('al-2', copy('in', { keepable: true }))]).every((g) => g.adding), true);
});

// What is left of a record: it stays while one copy does (docs/records-and-copies.md).
const held = (...copies: AlbumCopy[]) => ({ library: copies[0]?.library, format: copies[0]?.format ?? null, bitrate_kbps: copies[0]?.kbps ?? null, copies });

test('a record stays with its other copy when one goes, and its card reads what now plays', () => {
  const a = held(copy('music', { format: 'FLAC', kbps: 900, plays: true }), copy('own', { format: 'MP3', kbps: 320 }));
  const left = withoutCopy(a, 'music');
  assert.deepEqual([left?.library, left?.format, left?.bitrate_kbps, left?.copies?.map((c) => [c.library, c.plays])], ['own', 'MP3', 320, [['own', true]]]);
});

test('a copy that does not play leaves the card as it was', () => {
  const a = held(copy('music', { format: 'FLAC', kbps: 900, plays: true }), copy('in', { format: 'MP3', kbps: 128 }));
  assert.deepEqual([withoutCopy(a, 'in')?.format, withoutCopy(a, 'in')?.copies?.length], ['FLAC', 1]);
});

test('a record goes with its last copy', () => {
  assert.equal(withoutCopy(held(copy('music')), 'music'), null);
});

test('an album with no copies listed goes with its one library', () => {
  assert.equal(withoutCopy({ library: 'music', format: null, bitrate_kbps: null }, 'music'), null);
});

test('of two folders of one library only the first goes, as `from` names it', () => {
  const left = withoutCopy(held(copy('music', { format: 'FLAC' }), copy('music', { format: 'MP3' })), 'music');
  assert.deepEqual(left?.copies?.map((c) => c.format), ['MP3']);
});

test('a moved copy lies in its new library, and nothing leaves', () => {
  const moved = withCopyMoved(held(copy('in', { plays: true }), copy('own')), 'in', 'music');
  assert.deepEqual(moved.copies?.map((c) => c.library), ['music', 'own']);
  assert.equal(moved.library, 'music');
});

test('an album with no copies listed moves by its one library', () => {
  assert.equal(withCopyMoved({ library: 'in', format: null, bitrate_kbps: null }, 'in', 'music').library, 'music');
});

// Play from (docs/records-and-copies.md).
const on = (...off: string[]) => (l: string) => !off.includes(l);
const playFrom = (a: Parameters<typeof playFromEntries>[0], lit = on()) => playFromEntries(a, INFO, lit).map((e) => e.label);

test('Play from names each other copy by its library, an Incoming one too, and says which library it asks for', () => {
  const a = held(copy('music'), copy('more'), copy('in'));
  assert.deepEqual(playFromEntries(a, INFO, on()), [
    { library: 'more', label: 'Play from MORE' },
    { library: 'in', label: 'Play from IN' },
  ]);
});

test('the copy that plays has no entry, wherever the list puts it', () => {
  assert.deepEqual(playFrom(held(copy('in'), copy('music'))), ['Play from IN']);
});

test('the entries follow a switch: with the first copy off the next one on plays, and the first has none', () => {
  assert.deepEqual(playFrom(held(copy('music'), copy('more'), copy('own')), on('music')), ['Play from OWN']);
});

test('a copy in a library off has no entry, as a stream never plays it', () => {
  assert.deepEqual(playFrom(held(copy('music'), copy('in')), on('in')), []);
});

test('with the kept library off an Incoming copy is offered, as this is the one way to hear it', () => {
  assert.deepEqual(playFrom(held(copy('music'), copy('in')), on('music')), ['Play from IN']);
});

test('a record only in Incoming has no entry: its one copy plays', () => {
  assert.deepEqual(playFrom(held(copy('in', { plays: true }))), []);
});

test('a record of one copy has no entry, nor has a list that names no copies', () => {
  assert.deepEqual(playFrom(held(copy('music'))), []);
  assert.deepEqual(playFrom(held(copy('in'))), [], 'not even a lone copy the server has not said plays');
  assert.deepEqual(playFrom({ library: 'music' }), []);
  assert.deepEqual(playFrom({}), []);
});

test('two folders of one library are one entry, as `from` names a library', () => {
  assert.deepEqual(playFrom(held(copy('music'), copy('in'), copy('in'))), ['Play from IN']);
});

test('two folders of the library that plays are no other copy', () => {
  assert.deepEqual(playFrom(held(copy('music'), copy('music'))), []);
});

test('a library the list does not name goes by its id', () => {
  assert.deepEqual(playFrom(held(copy('music'), copy('gone'))), ['Play from gone']);
});

test('a switch thrown on the page moves the card to the next copy on', () => {
  const a = held(copy('music', { format: 'FLAC', kbps: 900 }), copy('more', { format: 'MP3', kbps: 320 }));
  assert.deepEqual([heardAs(a, INFO, on('music')).format, heardAs(a, INFO, on('music')).bitrate_kbps], ['MP3', 320]);
});

test('a record whose card is already true stays the same object, so a list of thousands costs nothing', () => {
  const a = held(copy('music', { format: 'FLAC', kbps: 900 }), copy('more', { format: 'MP3', kbps: 320 }));
  assert.equal(heardAs(a, INFO, on()), a);
  const single = held(copy('music'));
  assert.equal(heardAs(single, INFO, on('music')), single);
});

test('with no copy that can play the card keeps what the server said', () => {
  const a = held(copy('music', { format: 'FLAC' }), copy('in', { format: 'MP3' }));
  assert.equal(heardAs(a, INFO, on('music')), a);
});
