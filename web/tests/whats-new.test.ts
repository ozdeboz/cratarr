// UC03 (on UC02, the server's): a dot on Settings' info pill, and in About beside the version, while What's New holds something
// this account has not seen; a tap opens its sheet (UC-Q2 (a)) and marks the newest seen. The dot's rule runs; the pages are read.
// Red first: none of it was there at 7da786c91.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { newestOf, noteTitle, whatsNewDot } from '../src/lib/whatsNewRule.ts';

const src = (f: string) => readFileSync(new URL(`../src/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const note = (version: string, newer = false) => ({ version, newer, parts: [{ heading: null, lines: ['A line'] }] });

test('the dot shows while something is not seen, and only with notes to show', () => {
  assert.equal(whatsNewDot({ unread: true, notes: [note('0.18.0')] }), true);
  assert.equal(whatsNewDot({ unread: false, notes: [note('0.18.0')] }), false, 'seen: no dot');
  assert.equal(whatsNewDot({ unread: true, notes: [] }), false, 'nothing to open: no dot');
  assert.equal(whatsNewDot(null), false, 'not read (a server without it, or out of reach): no dot');
});

test('opened, the newest shown is what is marked seen; a newer release says so', () => {
  assert.equal(newestOf({ unread: true, notes: [note('0.19.0', true), note('0.18.0')] }), '0.19.0');
  assert.equal(newestOf({ unread: true, notes: [] }), null);
  assert.equal(noteTitle(note('0.18.0')), '0.18.0');
  assert.equal(noteTitle(note('0.19.0', true)), '0.19.0 · newer');
});

test("the info pill's About carries the dot; About's What's new, lit while unseen, opens the sheet and marks it seen", () => {
  assert.ok(src('Settings.svelte').includes("usePageAction(() => ({ icon: 'info', label: 'About', run: () => (about.open = true), dot: whatsNewShows() }));"));
  assert.ok(src('lib/ContentControls.svelte').includes('{#if a.dot}<span class="badge dot" aria-hidden="true"></span>{/if}'));
  const about = src('lib/AboutSheet.svelte');
  assert.ok(about.includes("label: \"What's new\",\n          on: whatsNewShows(),"));
  assert.ok(about.includes('end={notes}'));
  assert.match(about, /onclick: \(\) => \{\n            onclose\(\);\n            openWhatsNew\(\);/);
  const state = src('lib/whatsNew.svelte.ts');
  assert.match(state, /if \(!w \|\| !w\.unread \|\| !newest\) return;\n  whatsNew\.value = \{ \.\.\.w, unread: false \};\n  void whatsNewSeen\(newest\)/);
  assert.ok(src('App.svelte').includes('{#if whatsNew.open}<WhatsNewSheet onclose={() => (whatsNew.open = false)} />{/if}'));
});

test('the sheet shows the notes as the server parsed them, in the sheet every menu is, and nothing said after', () => {
  const sheet = src('lib/WhatsNewSheet.svelte');
  assert.ok(sheet.includes(`<MenuSheet label="What's new" title="What's new" icon="sparkles" {onclose}>`));
  assert.ok(sheet.includes('{#if part.heading}<h4>{part.heading}</h4>{/if}'));
  assert.doesNotMatch(src('lib/whatsNew.svelte.ts'), /toast/);
  const api = src('lib/api.ts');
  assert.ok(api.includes("quietly<import('./whatsNewRule').WhatsNew>('/api/whats-new')"));
  assert.ok(api.includes("post<unknown>('/api/whats-new/seen', { version })"));
});
