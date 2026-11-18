// A label's page (W11, C56; lib/labelPage): its subject in the panel, a record's label chip, a row's line, what "More" reads on,
// Discogs credited where its rows came from it, the page's picture, and a row opened by its record or its references with the label.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { fromDiscogs, labelChip, labelCovers, labelOf, labelSubject, moreToRead, rowLine, rowOpenable } = await import('../src/lib/labelPage.ts');
const { howToOpen } = await import('../src/lib/openRecord.ts');

const row = (r: Record<string, unknown> = {}) => ({ title: 'Selected Ambient Works 85-92', artist: 'Aphex Twin', had: false, ...r });

test("a label's subject, and back; nothing else is a label", () => {
  assert.equal(labelSubject('la-5'), 'label:la-5');
  assert.equal(labelOf(labelSubject('la-5')), 'la-5');
  for (const other of ['label:Warp', 'label:', 'album:cratarr/al-5', 'crate:label:Warp', null]) assert.equal(labelOf(other), null, String(other));
});

test("a record's label chip names its catalogue number where it has one", () => {
  assert.equal(labelChip({ id: 'la-5', name: 'Warp', catno: 'WARP123' }), 'Warp · WARP123');
  assert.equal(labelChip({ id: 'la-5', name: 'Warp' }), 'Warp');
  assert.equal(labelChip({ id: 'la-5', name: 'Warp', catno: 'none' }), 'Warp');
});

test("a row says its artist, year, catalogue number, and its type where it is not an album", () => {
  assert.equal(rowLine(row({ year: 1992, catno: 'AMB 3922', kind: 'Album' })), 'Aphex Twin · 1992 · AMB 3922');
  assert.equal(rowLine(row({ kind: 'EP' })), 'Aphex Twin · EP');
});

test('More reads on each source whose list does, MusicBrainz first', () => {
  assert.deepEqual(moreToRead({ more: { musicbrainz: true, discogs: true } }), ['musicbrainz', 'discogs']);
  assert.deepEqual(moreToRead({ more: { musicbrainz: false, discogs: true } }), ['discogs']);
  assert.deepEqual(moreToRead({ more: { musicbrainz: false, discogs: false } }), []);
});

test("Discogs is credited where a row came from it, not for MusicBrainz's", () => {
  assert.equal(fromDiscogs([row({ release_group: 'g1' })]), false);
  assert.equal(fromDiscogs([row({ release_group: 'g1', discogs_master: 4 })]), false, 'a record MusicBrainz knows too');
  assert.equal(fromDiscogs([row({ release_group: 'g1' }), row({ discogs_release: 99 })]), true);
});

test("the page's picture: its first four records' covers, else the first", () => {
  const c = (n: number) => row({ cover: `/c/${n}` });
  assert.deepEqual(labelCovers([c(1), row(), c(2), c(3), c(4), c(5)]), ['/c/1', '/c/2', '/c/3', '/c/4']);
  assert.deepEqual(labelCovers([c(1), c(2)]), ['/c/1']);
  assert.deepEqual(labelCovers([row()]), []);
});

test('a row opens its record, else is opened by its references and tied to the label', () => {
  assert.deepEqual(howToOpen(rowOpenable(row({ record: 'al-9', release_group: 'g1' })), 'la-5'), { record: 'al-9' });
  assert.deepEqual(howToOpen(rowOpenable(row({ discogs_release: 99, year: 1992 })), 'la-5'), {
    open: { discogs_release: 99, title: 'Selected Ambient Works 85-92', artist: 'Aphex Twin', year: 1992, label: 'la-5' },
  });
});
