// A record not had (C54, W9; lib/recordNotHad): its page from the catalogue, nothing of a copy's. Want is in its header, or
// Requested while its request is open; its Bandcamp preview plays from there (OQ14); one had through another record says so
// with no Want; a retired one says until when it was in Music (OQ16).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { albumRowSub, NOT_HAD_ENTRIES, notHadPage, offeredNotHad, wasInMusic } = await import('../src/lib/recordNotHad.ts');
const { MENUS } = await import('../src/lib/menuTable.ts');

const wanted = (state: string) => ({ state: { state } as never });

test('a record not had has no heart and no Resume; Want is in its header', () => {
  const p = notHadPage({ not_had: true });
  assert.equal(p.heart, false);
  assert.equal(p.resume, false);
  assert.deepEqual(p.actions, ['want']);
  assert.deepEqual(p.lines, []);
});

test('its Bandcamp preview plays from its header where it has one (OQ14), before Want', () => {
  assert.deepEqual(notHadPage({ not_had: true, bandcamp: 'https://a.bandcamp.com/album/b' }).actions, ['play', 'shuffle', 'next', 'last', 'want']);
  assert.deepEqual(notHadPage({ not_had: true, bandcamp: null }).actions, ['want']);
});

test('Requested while its request is open; Want again once it was declined, taken back or rejected', () => {
  for (const s of ['waiting_for_approval', 'wanted', 'partly_there', 'arrived']) assert.deepEqual(notHadPage({ request: wanted(s) }).actions, ['requested'], s);
  for (const s of ['declined', 'cancelled', 'rejected']) assert.deepEqual(notHadPage({ request: wanted(s) }).actions, ['want'], s);
  assert.deepEqual(notHadPage({ request: null }).actions, ['want']);
});

test('had through another record: In your library, opening that one, and no Want', () => {
  const p = notHadPage({ have: 'al-7', bandcamp: 'https://a.bandcamp.com/album/b' });
  assert.deepEqual(p.actions, ['play', 'shuffle', 'next', 'last']);
  assert.deepEqual(p.lines, [{ kind: 'have', record: 'al-7', text: 'In your library' }]);
});

test('a retired record says until when it was in Music (OQ16)', () => {
  const t = Date.UTC(2026, 2, 3, 12) / 1000;
  const line = wasInMusic(t);
  assert.equal(line, `Was in Music until ${new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' })}`);
  assert.match(line, /2026/);
  assert.deepEqual(notHadPage({ was_until: t }).lines, [{ kind: 'was', text: line }]);
});

test("its menu offers only what needs no copy: View, Go to, Share, the searches; each the album menu's own", () => {
  const album = MENUS.album.cards.flatMap((c) => c.entries);
  for (const e of NOT_HAD_ENTRIES) assert.ok(album.includes(e), e);
  assert.deepEqual([...NOT_HAD_ENTRIES].sort(), ['geniusSearch', 'goArtist', 'share', 'view', 'webSearch']);
  for (const e of ['favourite', 'rating', 'addToPlaylist', 'download', 'keep', 'edit', 'changeCover', 'moveTo', 'deleteAlbum'] as const) assert.ok(!offeredNotHad(e), e);
});

test('its rows play nothing and say no missing file; a copy short of a file still says so', () => {
  assert.equal(albumRowSub({ missing: true, artist: 'A' }, true, 'A'), undefined);
  assert.equal(albumRowSub({ missing: true, artist: 'B' }, true, 'A'), 'B');
  assert.equal(albumRowSub({ missing: true, artist: 'A' }, false, 'A'), 'Missing file');
  assert.equal(albumRowSub({ missing: false, artist: 'B' }, false, 'A'), 'B');
});
