// Export's choices (lib/exportChoices, decision 0058): the request they make for playlists and for a list of tracks,
// when it is ready, the line saying what it will hold, and the choices an older version kept.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { blankExport, exportLine, exportReady, exportRequest, keptExport, suggestedName } = await import('../src/lib/exportChoices.ts');

const tracks = { kind: 'tracks' as const, name: 'Kind of Blue', tracks: [{ source: 'cratarr', id: 'tr-1' }] };
const two = { kind: 'playlists' as const, names: ['Chill', 'robin/Jazz'] };

test('a list of tracks always carries them, its playlist file only when asked; a combined name only for several playlists', () => {
  const o = { ...blankExport(), tracks: false, combine: true, name: ' Both ', m3u: true, format: 'opus' as const, bitrate: 128, layout: 'flat' as const };
  assert.deepEqual(exportRequest(tracks, o), { what: tracks, format: 'opus', bitrate: 128, layout: 'flat', tracks: true, combine: null, m3u: true });
  assert.deepEqual(exportRequest(two, o), { what: two, format: 'opus', bitrate: 128, layout: 'flat', tracks: false, combine: 'Both', m3u: false });
  assert.equal(exportRequest({ kind: 'playlists', names: ['Chill'] }, o).combine, null, 'one playlist: nothing to combine');
});

test('no packaging to choose: the server makes one file itself, several one zip (the owner, 2026-10-30)', () => {
  assert.equal('package' in exportRequest(tracks, blankExport()), false);
});

test('ready with something to export and a combined playlist named', () => {
  const o = blankExport();
  assert.equal(exportReady(exportRequest(tracks, o)), true);
  assert.equal(exportReady(exportRequest({ ...tracks, tracks: [] }, o)), false);
  assert.equal(exportReady(exportRequest({ kind: 'playlists', names: [] }, o)), false);
  assert.equal(exportReady(exportRequest(two, { ...o, combine: true, name: '  ' })), false);
  assert.equal(exportReady(exportRequest(two, { ...o, combine: true, name: 'Both' })), true);
});

test('the line: how many tracks, about how large, how many missing are left out', () => {
  assert.equal(exportLine({ tracks: 312, missing: 3, bytes: 9.4e9, estimate: true }), '312 tracks, about 9.4 GB, 3 missing are left out');
  assert.equal(exportLine({ tracks: 1, missing: 1, bytes: 8_400_000, estimate: false }), '1 track, about 8 MB, 1 missing is left out');
  assert.equal(exportLine({ tracks: 12, missing: 0, bytes: null, estimate: false }), '12 tracks', 'only the playlist files: no size');
});

test('the combined name follows the playlists', () => {
  assert.equal(suggestedName(['A', 'B']), 'A + B');
  assert.equal(suggestedName(['A', 'B', 'C', 'D', 'E']), 'A + B and 3 more');
});

test("an older version's choices: one folder kept as the layout; anything not offered today at its default", () => {
  assert.deepEqual(keptExport({ tracks: true, flat: true, combine: false, name: 'X', named: true }), { ...blankExport(), tracks: true, name: 'X', named: true, layout: 'flat' });
  assert.deepEqual(keptExport({ format: 'mp3', bitrate: 64, layout: 'nested' }), blankExport());
  assert.deepEqual(keptExport({ format: 'flac-cd', bitrate: 256, layout: 'folders', flat: true }), { ...blankExport(), format: 'flac-cd', bitrate: 256 });
  assert.deepEqual(keptExport(null), blankExport());
});

test("0.10's choices kept: MP3 at 320 or V0, where Download… goes; a packaging kept before is let go", () => {
  assert.deepEqual(keptExport({ format: 'mp3-320', packaging: 'files', where: 'files' }), { ...blankExport(), format: 'mp3-320', where: 'files' });
  assert.equal(keptExport({ format: 'mp3-v0' }).format, 'mp3-v0');
  assert.deepEqual(keptExport({ where: 'cloud' }), blankExport(), 'nothing not offered');
  assert.equal(blankExport().where, 'device', 'to this device first, as Download once was');
});
