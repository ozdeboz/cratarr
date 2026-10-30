// A Bandcamp preview's ids as the page reads and writes them (lib/bandcampPreview), the server's way (bandcamp.rs
// track_id: "bc." and, base64url without padding, "<number>|<album page>").
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { albumIdOf, previewPage, samePage } = await import('../src/lib/bandcampPreview.ts');

test("an album's id is its track 0's, and reads back to its page", () => {
  const page = 'https://thieverycorporation.bandcamp.com/album/the-mirror-conspiracy';
  const id = albumIdOf(page);
  assert.equal(id, `bc.${Buffer.from(`0|${page}`).toString('base64url')}`);
  assert.equal(previewPage(id), page);
  // Not ASCII: written and read as UTF-8, as the server does.
  const odd = 'https://sigurros.bandcamp.com/album/ágætis-byrjun';
  assert.equal(previewPage(albumIdOf(odd)), odd);
  assert.equal(previewPage(`bc.${Buffer.from('3|' + page).toString('base64url')}`), page);
});

test('what is no preview reads as none; pages compare as pages', () => {
  assert.equal(previewPage('al-123'), null);
  assert.equal(previewPage('bc.!!'), null);
  assert.ok(samePage('https://a.bandcamp.com/album/x/', 'http://A.bandcamp.com/album/x?from=search'));
  assert.ok(!samePage('https://a.bandcamp.com/album/x', 'https://a.bandcamp.com/album/y'));
});
