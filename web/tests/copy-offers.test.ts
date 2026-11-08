// What each of a record's copies allows, and how its menu names them (lib/copiesRule, docs/records-and-copies.md, "On the
// page"): an offer per copy, a target already holding a copy is out, Replace from an upgrade into any main library
// holding a copy, and the entries saying which copy where a record has several.
import { test } from 'node:test';
import assert from 'node:assert/strict';

import type { AlbumCopy, AlbumsSource, LibraryInfo, Verdict } from '../src/lib/api.ts';

const { adds, deleteEntries, deleteGroups, heardAs, moveEntries, moveGroups, offersOf, playFromEntries, replaceEntries, withCopyMoved, withoutCopy } = await import('../src/lib/copiesRule.ts');

const library = (id: string, kind: LibraryInfo['kind'], more: Partial<LibraryInfo> = {}) => ({ id, name: id.toUpperCase(), kind, ...more }) as LibraryInfo;
const INFO = [library('music', 'main'), library('more', 'main'), library('in', 'incoming'), library('in2', 'incoming'), library('own', 'own', { mine: true }), library('theirs', 'own')];

const copy = (id: string, more: Partial<AlbumCopy> = {}) => ({ library: id, deletable: true, keepable: false, keep_to: null, ...more }) as AlbumCopy;
const list = (more: Partial<AlbumsSource> = {}) => ({ library_info: INFO, verdicts: {}, deletable: [], keepable: [], keep_to: {}, ...more }) as AlbumsSource;
const upgrade = (into: string[]): Record<string, Verdict> => ({ 'al-1': { kind: 'upgrade', library: 'A/B', why: '', library_id: into[0] ?? null, replaceable_into: into } });
const none = () => null;
const offers = (copies: AlbumCopy[], s = list(), admin = true) => offersOf(s, { id: 'al-1', copies }, admin, none);
const ids = (l: { id: string }[]) => l.map((x) => x.id);

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

test('a kept copy moves into any other library but own music', () => {
  assert.deepEqual(ids(offers([copy('music')])[0].targets), ['more', 'in', 'in2']);
});

test('only an administrator moves a kept copy, and no copy moves out of own music', () => {
  assert.deepEqual(offers([copy('music')], list(), false)[0].targets, []);
  assert.deepEqual(offers([copy('own')])[0].targets, []);
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

test("an upgrade replaces any main library's copy, its verdict's first; a duplicate replaces none", () => {
  const [incoming, music, more] = offers([copy('in'), copy('music'), copy('more')], list({ verdicts: upgrade(['more']) }));
  assert.deepEqual(ids(incoming.replaceInto), ['more', 'music']);
  assert.deepEqual([music.replaceInto, more.replaceInto], [[], []], 'a kept copy replaces nothing');
  const duplicate = list({ verdicts: { 'al-1': { ...upgrade(['music'])['al-1'], kind: 'duplicate' } } });
  assert.deepEqual(offers([copy('in'), copy('music')], duplicate)[0].replaceInto, []);
});

test('an Incoming copy may not replace what it may not delete', () => {
  assert.deepEqual(offers([copy('in', { deletable: false }), copy('music')], list({ verdicts: upgrade(['music']) }))[0].replaceInto, []);
});

test('the library a verdict replaces in is no target, though no copy there is listed with it', () => {
  const [o] = offers([copy('in', { keepable: true })], list({ verdicts: upgrade(['music']) }));
  assert.deepEqual(ids(o.targets), ['more', 'own']);
});

test("another server's album goes by what its list says of its id, in its one library", () => {
  const s = list({ deletable: ['al-1'], keepable: ['al-1'], keep_to: { 'al-1': 'more' } });
  const [o, ...rest] = offersOf(s, { id: 'al-1', library: 'in' }, true, none);
  assert.equal(rest.length, 0);
  assert.deepEqual([o.library, o.deletable, ids(o.targets)[0]], ['in', true, 'more']);
});

test('a copy its viewer may not delete goes nowhere', () => {
  assert.deepEqual(offers([copy('music', { deletable: false })])[0].targets, []);
});

test('a list with no libraries names no copy', () => {
  assert.equal(offersOf(list({ deletable: ['al-1'] }), { id: 'al-1' }, true, none)[0].library, undefined);
});

test('a record no one has has nothing to offer', () => {
  assert.deepEqual(offers([]), []);
});

test("each offer knows its own copy's job", () => {
  const o = offersOf(list(), { id: 'al-1', copies: [copy('in'), copy('music')] }, true, (l) => (l === 'in' ? 7 : null));
  assert.deepEqual(o.map((x) => x.job), [7, null]);
});

test('a copy with a job has no Move, Replace or Delete, and the copy beside it keeps them', () => {
  const o = offersOf(list({ verdicts: upgrade(['music']) }), { id: 'al-1', copies: [copy('in', { keepable: true }), copy('music')] }, true, (l) => (l === 'in' ? 7 : null));
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

test('Add to or Move to as the copy goes, each entry knowing its copy and its target', () => {
  const e = moveEntries(offers([copy('in', { keepable: true, keep_to: 'own' }), copy('music')]));
  assert.deepEqual(e.map((x) => [x.label, x.offer.library, x.to.id]), [
    ['Add to OWN', 'in', 'own'],
    ['Add to MORE', 'in', 'more'],
    ['Move to MORE', 'music', 'more'],
    ['Move to IN2', 'music', 'in2'],
  ]);
});

test('two copies that would say the same each name themselves, and their keys differ', () => {
  const e = moveEntries(offers([copy('music'), copy('more')], list({ library_info: [...INFO, library('third', 'main')] })));
  assert.deepEqual(e.filter((x) => x.to.id === 'third').map((x) => x.label), ['Move to THIRD from MUSIC', 'Move to THIRD from MORE']);
  assert.equal(new Set(e.map((x) => x.key)).size, e.length, 'a keyed each throws on a repeat');
});

test('Replace has an entry per main library, named by it', () => {
  const o = offers([copy('in'), copy('music'), copy('more')], list({ verdicts: upgrade(['music']) }));
  assert.deepEqual(replaceEntries(o).map((e) => [e.label, e.offer.library, e.to.id]), [
    ['Replace in MUSIC', 'in', 'music'],
    ['Replace in MORE', 'in', 'more'],
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
