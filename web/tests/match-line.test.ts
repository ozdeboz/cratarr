// What a search result's match line says (lib/search matchLine; the owner, 2026-10-31: "Hip Hop Music · 5 tracks" said
// neither where it was found nor why another line had no count), and when a searched list may say none match (listShows:
// never before the server's answer for the words as they are now).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { matchLine, listShows } = await import('../src/lib/search.ts');

test('the line names where the words were found', () => {
  assert.equal(matchLine([{ on: 'album', text: 'Hip Hop Music', count: 5 }], 'crate'), 'Album: Hip Hop Music · 5 tracks');
  assert.equal(matchLine([{ on: 'track', text: 'Livid Hip Hop', count: 1 }], 'playlist'), 'Track: Livid Hip Hop · 1 track');
  assert.equal(matchLine([{ on: 'artist', text: 'Miles Davis', count: 12 }], 'crate'), 'Artist: Miles Davis · 12 tracks');
  assert.equal(matchLine([{ on: 'composer', text: 'Johannes Brahms' }], 'album'), 'Composer: Johannes Brahms');
  assert.equal(matchLine([{ on: 'genre', text: 'hip hop' }], 'genre'), 'Genre: hip hop');
});

test('the count is said whenever the server gives one, a single one too', () => {
  assert.equal(matchLine([{ on: 'track', text: 'So What', count: 1 }], 'album'), 'Track: So What · 1 track');
  assert.equal(matchLine([{ on: 'track', text: 'So What', count: 1200 }], 'crate'), `Track: So What · ${(1200).toLocaleString()} tracks`);
  assert.equal(matchLine([{ on: 'artist', text: 'Jacco Gardner' }], 'track'), 'Artist: Jacco Gardner');
});

test('an artist found by an album counts albums', () => {
  assert.equal(matchLine([{ on: 'album', text: 'Kind of Blue', count: 2 }], 'artist'), 'Album: Kind of Blue · 2 albums');
});

test('found by its name: no line; the first other match leads', () => {
  assert.equal(matchLine([{ on: 'name', text: 'Hip Hop' }]), null);
  assert.equal(matchLine([]), null);
  assert.equal(matchLine([{ on: 'name', text: 'X' }, { on: 'track', text: 'Y', count: 3 }], 'playlist'), 'Track: Y · 3 tracks');
});

test('a list says none match only once the words as they are now are answered', () => {
  assert.equal(listShows(3, 'hip hop', null), 'rows');
  assert.equal(listShows(0, 'hip hop', null), 'asking');
  assert.equal(listShows(0, 'hip hop', 'hip ho'), 'asking');
  assert.equal(listShows(0, ' hip hop ', 'hip hop'), 'empty');
  // No words: nothing asked, so the filter's own none is said at once.
  assert.equal(listShows(0, '  ', ''), 'empty');
});
