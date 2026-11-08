// A record's menu closes as its Play is tapped, and Play goes on reading what the menu was opened on (0.16.0 on the tablet,
// 2026-11-08: Play in a synced album's menu said "Could not start playing", the record's details read from a menu already gone).
// Every opener that hands the record menu a value that closes keeps it as opened: Downloads, search, Health, a work. Read.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (f: string) => readFileSync(new URL(`../../src/lib/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test('Downloads reads the menu as opened, kept while it closes, never the closed one', () => {
  const src = read('DownloadsPanel.svelte');
  assert.ok(src.includes('const shownMenu = $derived((opened = menu ?? opened));'));
  assert.ok(src.includes('{#if menu}\n  {@const m = shownMenu!}'), 'its entries and the record menu read the kept one');
  assert.ok(!src.includes('{@const m = menu}'));
});

test('search, Health and a work hand the record menu the value as opened', () => {
  const search = read('GlobalResults.svelte');
  assert.ok(search.includes('const shownMenu = $derived((opened = menu ?? opened));'));
  assert.ok(search.includes("{@const m = shownMenu as Extract<Menu, { kind: 'album' }>}\n  <AlbumMenu source={m.source} album={{ id: m.id"));
  const health = read('HealthPanel.svelte');
  assert.ok(health.includes('const shownCover = $derived((openedCover = coverOf ?? openedCover));'));
  assert.ok(health.includes('{@const c = shownCover!}\n  <AlbumMenu source="cratarr" album={{ id: c.id'));
  const work = read('WorkSheet.svelte');
  assert.ok(work.includes('const shownAlbum = $derived((openedAlbum = albumMenu ?? openedAlbum));'));
  assert.ok(work.includes('{#if albumMenu}{@const r = shownAlbum!}<AlbumMenu'));
});
