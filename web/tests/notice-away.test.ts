// One offline mode for what failed (lib/noticeAway, lib/Notice; 0.17, the owner: "a unified way of handling offline mode"): out of
// reach, a page the device cannot answer says so in one line in place of its Notice and Try again; reached, a failure keeps its Notice.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { noticeShows } = await import('../src/lib/noticeAway.ts');

test('reached, a failure is its Notice with Try again, whatever it would say away', () => {
  assert.equal(noticeShows(false, 'Discover needs Cratarr.'), 'notice');
  assert.equal(noticeShows(false, ''), 'notice');
  assert.equal(noticeShows(false, undefined), 'notice');
});

test('out of reach, its one line; a part of a page whose line is said elsewhere, nothing; one with no line, its Notice', () => {
  assert.equal(noticeShows(true, 'Discover needs Cratarr.'), 'line');
  assert.equal(noticeShows(true, ''), 'none');
  assert.equal(noticeShows(true, undefined), 'notice', 'a sheet’s or a menu’s keeps it');
});

// The pages and their parts the device cannot answer: each failure there has its line out of reach.
const src = (path: string) => readFileSync(new URL(`../src/${path}`, import.meta.url), 'utf8');
const PAGES = [
  'Discover.svelte',
  'Crates.svelte',
  'Health.svelte',
  'Requests.svelte',
  'Playlists.svelte',
  'lib/InsightsPanel.svelte',
  'lib/RequestsPanel.svelte',
  'lib/HealthPanel.svelte',
  'lib/HealthEvents.svelte',
  'lib/HealthMissing.svelte',
  'lib/HealthReconnect.svelte',
  'lib/FileChanges.svelte',
  'lib/GlobalResults.svelte',
  'lib/AlbumShelf.svelte',
  'lib/HomeStatus.svelte',
  'lib/ArtistsToExplore.svelte',
  'lib/settings/DiagnosticsPart.svelte',
  'lib/settings/UsersPart.svelte',
  'lib/settings/WordsPart.svelte',
];

test('every failure on a page the device cannot answer says its one line out of reach, never Try again', () => {
  for (const page of PAGES) {
    // A failure: its Notice with Try again, or the one Insights shows. Not a finding's words (Health's), nor a server's mode's rows
    // (decision 0098: that server's, not Cratarr's).
    const notices = (src(page).match(/<Notice\b[\s\S]*?\/>/g) ?? []).filter((x) => (x.includes('onretry') || x.includes('insights.error')) && !x.includes('fromServer'));
    assert.ok(notices.length, `${page} has its failure`);
    for (const n of notices) assert.match(n, /\baway=/, `${page}: ${n}`);
  }
});
