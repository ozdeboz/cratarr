// F28: a record's health line opens a sheet in place. What can be done for an item is one when-table (lib/healthTools), drawn by
// Health's row and the record's sheet alike (lib/HealthItemTools); the sheet is mounted on the record's page, never a step.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { itemTools, LEVEL, sheetFor } from '../src/lib/healthTools.ts';

const src = (f: string) => readFileSync(new URL(`../src/lib/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const item = (o: object) => ({ title: 'Night Ferry', ...o }) as never;

test("F28.2: what can be done for an item, by one when-table, in today's order", () => {
  assert.deepEqual(itemTools('library:references', item({ album: ['cratarr', 'al-42'] }), { admin: true }), ['references', 'goTo']);
  assert.deepEqual(itemTools('playlists:missing', item({ playlist: 'p1' })), ['playlistMissing', 'goTo']);
  assert.deepEqual(itemTools('library:missing', item({ track: 't1' })), ['reconnect']);
  assert.deepEqual(itemTools('incoming:waiting', item({ album: ['cratarr', 'al-3'] })), ['incoming', 'goTo']);
  assert.deepEqual(itemTools('library:track-numbers', item({ album: ['cratarr', 'al-4'] }), { renumber: true }), ['renumber', 'goTo']);
  assert.deepEqual(itemTools('library:track-numbers', item({ album: ['cratarr', 'al-4'] }), { renumber: false }), ['goTo'], 'Renumber where offered');
  assert.deepEqual(itemTools('server:slow', item({})), ['events']);
  assert.deepEqual(itemTools('library:incomplete', item({ album: ['cratarr', 'al-5'], missing: ['Track 11'] })), ['missingTracks', 'askFull', 'goTo']);
  assert.deepEqual(itemTools('library:unheard', item({ files: ['a.flac'] })), ['files']);
  assert.deepEqual(itemTools('files-short', item({ line: '/music/a.flac: cut off' }), { fixable: true }), ['fileLine', 'fixes']);
  assert.deepEqual(itemTools('files-broken', item({ track: 't9', album: ['cratarr', 'al-9'] }), { finds: true }), ['betterCopy', 'goTo']);
  assert.deepEqual(itemTools('library:cover', item({ album: ['cratarr', 'al-6'] }), { admin: true }), ['changeCover', 'goTo']);
  assert.deepEqual(itemTools('library:cover', item({ album: ['cratarr', 'al-6'] }), { admin: false }), ['goTo'], "an administrator's alone");
  assert.deepEqual(LEVEL, { critical: 'Critical', warning: 'Warning', info: 'To know' });
});

test('F28.3: no U-turn: the sheet over a record leaves out its Go to; Health keeps it', () => {
  const x = item({ album: ['cratarr', 'al-42'], missing: ['Track 11'] });
  assert.ok(!itemTools('library:incomplete', x, { here: 'al-42' }).includes('goTo'));
  assert.ok(itemTools('library:incomplete', x).includes('goTo'));
  assert.ok(itemTools('library:incomplete', x, { here: 'al-7' }).includes('goTo'), 'another record: its Go to');
});

test("F28.5, F28.6: the sheet's finding is the tapped line's, as the health reads now; gone, none", () => {
  const findings = [{ key: 'library:references' }, { key: 'files:short' }];
  const keyOf = (f: { key: string }) => (f.key.startsWith('files:') ? `files-${f.key.slice(6)}` : f.key);
  assert.deepEqual(sheetFor(findings, 'files-short', keyOf), { key: 'files:short' });
  assert.equal(sheetFor(findings, 'library:incomplete', keyOf), null, 'fixed or gone: the sheet closes');
  assert.equal(sheetFor(null, 'library:references', keyOf), null, 'not read: nothing opens');
  assert.equal(sheetFor(findings, null, keyOf), null, 'no key tapped: nothing opens');
});

test('F28.1, F28.4, F28.6: the line opens the sheet over the page; a tool that opens its own sheet takes its place', () => {
  const view = src('PlaylistView.svelte');
  assert.match(view, /onclick=\{\(\) => openRecordProblem\(l\)\}/);
  assert.match(view, /const openRecordProblem = \(l: ReturnType<typeof recordProblem>\) => album && \(problemOpen \?\?= \{/, 'the first tap wins; no step');
  assert.doesNotMatch(view, /viewOver\(`\$\{HEALTH\}/, 'never a health: step');
  assert.match(view, /<RecordProblemSheet\n    finding=\{problemFinding\}/);
  assert.match(view, /if \(problemOpen && !problemFinding\) untrack\(\(\) => \(problemOpen = null\)\);/, 'a finding gone closes it');
  const sheet = src('RecordProblemSheet.svelte');
  assert.match(sheet, /\{#if searchOf\}[\s\S]*\{:else if betterOf\}[\s\S]*\{:else if coverOf\}[\s\S]*\{:else if !elsewhere\}\n  <MenuSheet/, 'never two sheets stacked');
  assert.match(sheet, /if \(review\.thing\) onclose\(\);/, 'Review the fixes: the one review sheet, in its place');
  assert.match(sheet, /LEVEL\[finding\.level\]/);
  assert.match(sheet, /\{#if finding\.detail\}<MoreInfo/);
  assert.doesNotMatch(sheet, /location\.hash|viewOver/);
});

test('F28.2: Health and the sheet draw one piece; Health keeps no tools of its own', () => {
  const health = src('HealthPanel.svelte');
  assert.match(health, /<HealthItemTools\n    \{problem\}/);
  assert.match(src('RecordProblemSheet.svelte'), /<HealthItemTools\n          \{problem\}/);
  assert.doesNotMatch(health, /HealthMissing|HealthReconnect|HealthIncoming|HealthRenumber|HealthEvents|RecordReferences/);
  assert.match(health, /import \{ LEVEL, allGone, fullOf, goneTracks \} from '\.\/healthTools';/, 'the level words from one map (LS05: and All gone by its rule)');
  assert.match(src('HealthItemTools.svelte'), /itemTools\(problem, x, \{/);
});
