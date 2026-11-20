// CU23 with CU22's page side: Basic hides rows (the owner, 2026-11-06; Q5, Q6). One `partsShown()` for the five places that each
// decided which parts show; a row hides through lib/settings/narrow as the search hides one; the search reads lib/settings/tiers; a
// part opened by its address shows whole; Reset lists only what shows; the foot line says what is set out of sight; one switch per
// account, an icon lit while on Advanced. Red first: none of it was there at 7da786c91. Pure rules run; the pages, which take a
// whole app, are read.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
(globalThis as Record<string, unknown>).$effect = () => {};
const { partsFor, partsShown } = await import('../src/lib/settingsSections.svelte.ts');

const source = (f: string) => readFileSync(new URL(`../src/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const ids = (list: { id: string }[]) => list.map((x) => x.id);

test('Advanced lists every part as before; Basic leaves out the parts with no Basic row, and never a server part', () => {
  for (const admin of [false, true])
    for (const app of [false, true]) assert.deepEqual(ids(partsShown(admin, app, false, false)), ids(partsFor(admin, app, false)));
  assert.deepEqual(ids(partsShown(false, true, false, true)), ['account', 'taste', 'tracking', 'players', 'device', 'network', 'sync', 'appearance']);
  const admin = ids(partsShown(true, true, false, true));
  for (const gone of ['radio', 'import', 'layout', 'player', 'content']) assert.ok(!admin.includes(gone), gone);
  for (const stays of ['connections', 'libraries', 'players', 'users', 'auth', 'serverimport', 'diagnostics']) assert.ok(admin.includes(stays), stays);
});

test('a row hides in Basic through the narrowing the search uses: its `left`', () => {
  const item = source('lib/settings/Item.svelte');
  assert.ok(item.includes('const basicOut = basicLeaves();'));
  assert.ok(item.includes('const left = $derived(basicOut(name) || (narrowed() && !groupHas() && !matchesSearch(name, shown(el))));'));
  const narrow = source('lib/settings/narrow.ts');
  assert.ok(narrow.includes('return p !== null && !basicShows(p, name);'));
});

test('a group goes whole where its title is Advanced, and where every row of it is left out', () => {
  const group = source('lib/settings/Group.svelte');
  assert.ok(group.includes('class:basic={basic.on()} class:out={basic.out(title)}'));
  assert.match(group, /\.group\.out,\n  \.group\.basic:has\(:global\(\.item\)\):not\(:has\(:global\(\.item:not\(\.left\)\)\)\) \{\n    display: none;/);
});

test('only a part reached from the list or its pickers is narrowed: a thing, or a part opened by its address, shows whole', () => {
  const panel = source('lib/SettingsPanel.svelte');
  assert.ok(panel.includes("const basicPart = $derived(settingsBasic() && !thing && settingsListed.part === section && TIERED.includes(section) && listedInBasic(section) ? section : null);"));
  assert.ok(panel.includes('basicSettings(() => basicPart);'));
  // Each way within Settings says so: the list, the pickers, the search through the other parts, the part opened beside the list.
  assert.ok(source('Settings.svelte').includes('onclick={() => ((settingsListed.part = c.id), viewPlaylist(SETTINGS + c.id))}'));
  assert.match(panel, /const toPart = \(id: string\) => \{\n    settingsListed\.part = id as SettingsSection;\n    viewSibling\(SETTINGS \+ id\);/);
  assert.ok(panel.includes('onchoose: toPart }'));
  assert.ok(panel.includes('if (to) toPart(to);'));
  assert.ok(panel.includes('<Item name={row} onclick={() => toPart(e.id)} />'));
  assert.ok(source('App.svelte').includes('if (part) untrack(() => ((settingsListed.part = part.id), viewInPlace(SETTINGS_VIEW + part.id)));'));
});

test('Reset lists only what shows, and the foot line says how many more are set out of sight', () => {
  const panel = source('lib/SettingsPanel.svelte');
  assert.ok(panel.includes('const resetShows = $derived(basicPart ? (name: string) => basicResets(section, name) : undefined);'));
  assert.ok(panel.includes('rows={resetRows(section, resetShows)} onconfirm={() => runReset(section, resetShows)}'));
  assert.ok(panel.includes('const moreSet = $derived(basicPart ? resetRows(section).length - resetRows(section, resetShows).length : 0);'));
  assert.ok(panel.includes('{#if moreSet}<p class="foot">{moreSet} more set in Advanced</p>{/if}'));
  assert.equal(panel.match(/resetRows\(section\)/g)?.length, 1, 'only the foot line counts what Basic hides');
});

test('the search reads the tiers: in Basic it finds no row Basic hides, nor a group that goes', () => {
  const search = source('lib/settingsSearch.svelte.ts');
  assert.ok(search.includes("const shows = (s: string) => !basic || (basicShows(part, s) && groupTier(part, s) !== 'advanced');"));
  assert.ok(source('Settings.svelte').includes('settingsMatches(id, title, words, settingsBasic())'));
  assert.ok(source('lib/everywhere.svelte.ts').includes('settingsMatches(x.id, x.title, query, basic)'));
  assert.ok(source('lib/SettingsPanel.svelte').includes('settingsMatches(x.id, x.title, w, basic)'));
});

test("one switch per account, kept on it, Advanced until it says; an icon lit while on Advanced, the list's on a phone", () => {
  assert.ok(source('lib/accountPrefs.svelte.ts').includes("settings_advanced: 'settingsAdvanced',"));
  const prefs = source('lib/prefs.svelte.ts');
  assert.ok(prefs.includes('  settingsAdvanced: null,\n'), 'none held: Advanced');
  assert.ok(prefs.includes("'source', 'settingsAdvanced'] as const;"), "the account's own");
  assert.ok(source('lib/settings/basic.svelte.ts').includes('export const settingsBasic = () => prefs.settingsAdvanced === false;'));
  const panel = source('lib/SettingsPanel.svelte');
  assert.ok(panel.includes('<button class="advanced" aria-label="Advanced" title="Advanced" aria-pressed={!settingsBasic()} onclick={toggleAdvanced}><Icon name="advanced" size={22} /></button>'));
  assert.ok(source('Settings.svelte').includes("usePageMode(() => (glass.one ? { icon: 'advanced', label: 'Advanced', on: !settingsBasic(), toggle: toggleAdvanced } : null));"));
});
