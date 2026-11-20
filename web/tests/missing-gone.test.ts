// LS05 (library safety's page; LS02–LS04 the server's): a gone track of Missing music gets "Gone", the finding "All gone" in red, and a
// held library "They are gone"; each asked once more by ConfirmSheet, with no word after. Which items offer Gone is the when-table's
// (lib/healthTools), the rule test; the pages are read. Red first: none of it was there at 7da786c91.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { allGone, goneTracks, itemTools } from '../src/lib/healthTools.ts';

const src = (f: string) => readFileSync(new URL(`../src/lib/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const item = (o: object) => ({ title: 'Night Ferry', ...o }) as never;

test('a gone track offers Gone to an administrator, after its reconnecting; a user, and anything but a track, none', () => {
  assert.deepEqual(itemTools('library:missing', item({ track: 't1' }), { admin: true }), ['reconnect', 'gone']);
  assert.deepEqual(itemTools('library:missing', item({ track: 't1' }), { admin: false }), ['reconnect'], "an administrator's word");
  // A parked library, or a retired track someone holds: no track of its own to let go.
  assert.deepEqual(itemTools('library:missing', item({ line: 'Music: not mounted' }), { admin: true }), ['fileLine']);
  assert.ok(!itemTools('library:references', item({ track: 't1', album: ['cratarr', 'al-1'] }), { admin: true }).includes('gone'));
});

test('a held library offers They are gone to an administrator, by its folder', () => {
  assert.deepEqual(itemTools('library:held', item({ folder: '/media/music' }), { admin: true }), ['heldGone']);
  assert.deepEqual(itemTools('library:held', item({ folder: '/media/music' }), { admin: false }), []);
  assert.deepEqual(itemTools('library:held', item({}), { admin: true }), [], 'no folder, nothing to let through');
});

test('All gone: the finding lets go every track it lists, and nothing else, to an administrator', () => {
  const items = [item({ track: 't1' }), item({ line: 'Music: not mounted' }), item({ track: 't2' })];
  assert.deepEqual(goneTracks(items), ['t1', 't2']);
  assert.equal(allGone('library:missing', items, true), true);
  assert.equal(allGone('library:missing', items, false), false);
  assert.equal(allGone('library:missing', [item({ line: 'Music: not mounted' })], true), false, 'no track: no All gone');
  assert.equal(allGone('library:held', items, true), false);
});

test('each is asked by ConfirmSheet, the act in red, and nothing said after it', () => {
  const tools = src('HealthItemTools.svelte');
  assert.ok(tools.includes(`{#if has('gone')}<Item name="Gone" kind="danger" onclick={() => (goneAsk = 'gone')} />{/if}`));
  assert.ok(tools.includes(`{#if has('heldGone')}<Item name="They are gone" kind="danger" onclick={() => (goneAsk = 'heldGone')} />{/if}`));
  assert.ok(tools.includes('label="Gone" onconfirm={() => void gone(\'gone\')}'));
  assert.ok(tools.includes('label="They are gone"'));
  assert.match(tools, /if \(kind === 'gone'\) await missingGone\(\[x\.track!\]\);\n      else await missingAccept\(x\.folder!\);\n      ongone\(\);/);
  const panel = src('HealthPanel.svelte');
  assert.ok(panel.includes('{#if f && allGone(f.key, p.items, me.admin)}\n        <Group><Item name="All gone" kind="danger" onclick={() => (allGoneAsk = true)} /></Group>'));
  assert.ok(panel.includes('label="All gone"\n    onconfirm={() => void letAllGo(tracks)}'));
  for (const s of [tools, panel]) assert.doesNotMatch(s, /toast\.show\(\{ text: [^}]*gone/i, 'no word after');
});

test('it asks the server: the tracks let go, the held walk let through', () => {
  const api = src('api.ts');
  assert.ok(api.includes("post<unknown>('/api/missing/gone', { tracks })"));
  assert.ok(api.includes("post<unknown>('/api/missing/accept', { folder })"));
});
