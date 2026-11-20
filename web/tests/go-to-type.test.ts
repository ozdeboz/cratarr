// GOTO: a track's Go to names its record's type, which the server sends with the track (album_type, album_secondary). Each
// MenuTrack builder carries the pair from the answer it was built from; TrackMenu and SelectionMenu name it by `goToName`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { goToName } from '../src/lib/menuTable.ts';

const src = (f: string) => readFileSync(new URL(`../src/lib/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test('GOTO.2: a track row brings its record\'s type to its menu', () => {
  assert.match(src('TrackMenu.svelte'), /album_type\?: string \| null;\n    album_secondary\?: string\[\];/, 'MenuTrack carries the pair');
  assert.match(src('TrackMenu.svelte'), /label=\{goToName\(t\.album_type, t\.album_secondary\)\}/);
  for (const [file, from] of [
    ['PlaylistView.svelte', 'r'],
    ['CrateResults.svelte', 'r'],
  ] as const) {
    const s = src(file);
    assert.ok(s.includes(`album_type: ${from}.album_type ?? null,\n    album_secondary: ${from}.album_secondary ?? [],`), `${file}: its MenuTrack builder`);
  }
  assert.ok(src('QueueSheet.svelte').includes('album_type: t.album_type ?? null,\n    album_secondary: t.album_secondary ?? [],'), "the queue's (GOTO.4)");
  assert.match(src('api.ts'), /export type PlaylistRow = \{\n  \/\*\* Its record's type[^*]*\*\/\n  album_type\?: string \| null;\n  album_secondary\?: string\[\];/);
  assert.match(src('generated/QueueTrack.ts'), /album_type\?: string, album_secondary\?: Array<string>/, 'as ts-rs writes it');
});

test('GOTO.4 (a): a track queued from a row keeps the row\'s type', () => {
  assert.match(src('localPlayer.svelte.ts'), /album_type\?: string \| null;\n  album_secondary\?: string\[\];/, 'LocalTrack carries the pair');
  assert.ok(src('PlaylistView.svelte').includes('album_type: x.album_type ?? null,\n      album_secondary: x.album_secondary ?? [],'), 'tracksOf');
  assert.ok(src('CrateResults.svelte').includes('album_type: r.album_type ?? null,\n        album_secondary: r.album_secondary ?? [],'), 'localOf');
});

test("GOTO.3: a record's menu in a list names its rows' type", () => {
  assert.match(src('SelectionMenu.svelte'), /label=\{goToName\(albumType, albumSecondary\)\}/);
  assert.match(src('PlaylistView.svelte'), /albumType=\{rs\.find\(\(r\) => r\.album_type\)\?\.album_type \?\? null\}/);
  assert.match(src('CrateResults.svelte'), /albumType=\{rows\.find\(\(r\) => r\.album_type\)\?\.album_type \?\? null\}/);
});

test('GOTO.7, GOTO.4 (b): "Go to record" only where the type is truly unknown; a queue kept from before reads it', () => {
  assert.equal(goToName('ep', []), 'Go to EP');
  assert.equal(goToName('single'), 'Go to single');
  assert.equal(goToName('album', ['compilation']), 'Go to compilation');
  assert.equal(goToName(undefined, undefined), 'Go to record', 'no type sent: a queue kept from before the upgrade');
  assert.equal(goToName(null, []), 'Go to record');
});
