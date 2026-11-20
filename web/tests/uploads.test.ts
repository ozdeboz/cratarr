// UP04, the upload sheet (SCENARIOS-RUNS-13-14 UP04.1–UP04.6; UP01–UP03 the server's): lib/uploads' rules run, and the sheet's source
// is read where it takes a whole app. Red first: neither lib/uploads nor lib/UploadSheet was there at 7da786c91.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { lineProgress, matchPicked, nextPiece, pickedFolder, pickRefusal, pieceDigest, sendFrom, wholeOf } from '../src/lib/uploads.ts';

const src = (f: string) => readFileSync(new URL(`../src/lib/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const file = (name: string, size: number) => ({ name, size });

test("UP04.1: the next piece starts at the server's count; a whole file is done; a count past its size is an error; the last piece is short", () => {
  assert.deepEqual(nextPiece({ size: 20 }, 8, 8), { start: 8, end: 16 });
  assert.equal(nextPiece({ size: 20 }, 20, 8), null);
  assert.throws(() => nextPiece({ size: 20 }, 24, 8));
  assert.deepEqual(nextPiece({ size: 20 }, 16, 8), { start: 16, end: 20 });
  assert.deepEqual(nextPiece({ size: 20 }, 0, 8), { start: 0, end: 8 });
});

test('UP04.2: after a cut it goes on from the status, whether the piece sent landed or not; the line follows the status', async () => {
  // Piece 4 (24 to 32) sent, its answer never came: the status says 24 (it did not land) or 32 (it did).
  assert.deepEqual(nextPiece({ size: 40 }, 24, 8), { start: 24, end: 32 });
  assert.deepEqual(nextPiece({ size: 40 }, 32, 8), { start: 32, end: 40 });
  // Sent from where the server says, each piece's offset the server's count after the one before.
  const sent: number[] = [];
  const blob = new Blob([new Uint8Array(20)]);
  const end = await sendFrom(blob, 8, async (offset, piece) => (sent.push(offset), offset + piece.size), 8);
  assert.deepEqual(sent, [8, 16]);
  assert.equal(end, 20);
  // The server counting less than was sent (a torn piece cut back): the next goes from its count, not the page's.
  const told: number[] = [];
  const counts = [8, 8, 16, 20];
  await sendFrom(blob, 0, async (offset) => (told.push(offset), counts.shift()!), 8);
  assert.deepEqual(told, [0, 8, 8, 16]);
  assert.equal(lineProgress({ size: 40, received: 24 }), 0.6, 'the status, never the bytes sent');
});

test('UP04.3: after a reload the upload is listed with its whole files, and its files picked again match by name and size', () => {
  const picked = [file('a.flac', 10), file('a.flac', 11)];
  assert.deepEqual(matchPicked([file('a.flac', 10)], picked), [picked[0]]);
  assert.deepEqual(matchPicked([file('a.flac', 12)], picked), [null], 'another size is no match, and is not sent');
  const two = [file('01.flac', 5), file('02.flac', 5)];
  assert.deepEqual(matchPicked(two, [two[1], two[0]]), [two[0], two[1]], 'by the upload\'s order');
  assert.equal(wholeOf([{ size: 5, received: 5 }, { size: 5, received: 5 }, { size: 5, received: 5 }, ...Array(7).fill({ size: 5, received: 1 })]), '3 of 10 files');
  const sheet = src('UploadSheet.svelte');
  assert.ok(sheet.includes('<Item name={u.folder ?? \'Upload\'} line={wholeOf(u.files)} onclick={() => pickAgain(u)} />'));
  assert.ok(sheet.includes('handles = matchPicked(again.files, picked);'));
});

test('UP04.4: the whole pick is checked first; the server\'s refusal is shown as it is; below the floor, the server is short of space', () => {
  assert.equal(pickRefusal(1.4e9, 1e9, false), 'Needs 1.4 GB, 1 GB left');
  assert.equal(pickRefusal(0.5e9, 1e9, false), null);
  assert.equal(pickRefusal(9e12, null, false), null, 'no room: an administrator');
  assert.equal(pickRefusal(1, 1e9, true), 'The server is short of space');
  const sheet = src('UploadSheet.svelte');
  assert.match(sheet, /refusal = pickRefusal\(\n      picked\.reduce\(\(n, f\) => n \+ f\.size, 0\),\n      room \? room\.size - room\.used : null,/);
  assert.match(sheet, /if \(refusal\) return;\n    try \{\n      upload = await createUpload\(/, 'nothing sent when refused');
  assert.match(sheet, /\} catch \(e\) \{\n      \/\/ The server's own words, as they are[^\n]*\n      refusal = e instanceof Error \? e\.message : String\(e\);/);
});

test("UP04.5: a line each, then the records placed, each opening in Incoming; no notice; the folder's name, never a path", () => {
  assert.equal(pickedFolder([{ webkitRelativePath: 'Tide Tables/01.flac' }, { webkitRelativePath: 'Tide Tables/cover.jpg' }]), 'Tide Tables');
  assert.equal(pickedFolder([{ webkitRelativePath: '' }, { webkitRelativePath: '' }]), null, 'files picked loose');
  assert.equal(pickedFolder([{ webkitRelativePath: 'A/x.flac' }, { webkitRelativePath: 'B/y.flac' }]), null);
  const sheet = src('UploadSheet.svelte');
  assert.ok(sheet.includes('{#each upload.placed as p (p.album)}<Item name={p.title} onclick={() => open(p.album)} />{/each}'));
  assert.ok(sheet.includes("viewOver(albumSubject('cratarr', album));"));
  assert.doesNotMatch(sheet, /toast/, 'no success notice');
  // The pieces every sheet is made of, and none of its own.
  const imports = [...sheet.matchAll(/import (\w+) from '\.\/([\w/]+)\.svelte';/g)].map((m) => m[2]);
  assert.deepEqual(imports.sort(), ['MenuEntry', 'MenuSheet', 'Notice', 'settings/Group', 'settings/Item']);
  assert.ok(src('api.ts').includes("post<Upload>('/api/uploads', { folder, files })"));
});

test('UP04.6: a piece at a time: each sliced from its file and hashed alone; nothing reads a whole file', async () => {
  const rules = src('uploads.ts');
  assert.ok(rules.includes('const piece = file.slice(next.start, next.end);'));
  assert.ok(rules.includes("crypto.subtle.digest('SHA-256', await piece.arrayBuffer())"));
  assert.equal(rules.match(/arrayBuffer\(\)/g)?.length, 1, "the piece's alone");
  assert.equal(await pieceDigest(new Blob(['abc'])), 'sha-256=:ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=:');
  assert.ok(src('api.ts').includes("headers: { 'Content-Type': 'application/octet-stream', 'Content-Digest': digest },"));
});
