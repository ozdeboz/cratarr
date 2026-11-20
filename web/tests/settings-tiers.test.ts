// CU21: each row of Personal's and This device's parts has its tier declared once (lib/settings/tiers), read again at trunk from the
// parts themselves: every row a tier, every tier a row, and the owner's Basic rows Basic (the owner's calls of 2026-11-06, plus
// Mobile data). Red first: tiers.ts did not exist.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
(globalThis as Record<string, unknown>).$effect = () => {};
const { SECTIONS } = await import('../src/lib/settingsSections.svelte.ts');
const { ADVANCED_PARTS, GROUPS, ROWS, TIERED, basicResets, basicShows, groupTier, listedInBasic, rowTier } = await import('../src/lib/settings/tiers.ts');

const source = (f: string) => readFileSync(new URL(`../src/lib/settings/${f}.svelte`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

/** The files each tiered part's rows are written in: its own, and the groups of rows it shows (a thing's own page apart:
 *  SpotifyPart, ServerPage, which show whole). The Advanced parts' rows are all Advanced, so none is listed. */
const FILES: Record<string, string[]> = {
  account: ['AccountPart'],
  taste: ['TastePart'],
  tracking: ['TrackingPart', 'ListeningPart', 'DeviceListening'],
  device: ['DevicePart', 'LevelItems', 'TapeItems', 'VisualsAutoGroup', 'ServersGroup'],
  network: ['NetworkPart'],
  sync: ['SyncPart'],
  appearance: ['AppearancePart'],
};
/** A part's rows named in its markup: `<Item name="…"`, `<ChoiceRow name="…"`, `<SettingRow name="…"`. */
const rowsOf = (part: string) =>
  FILES[part].flatMap((f) => [...source(f).matchAll(/<(?:Item|ChoiceRow|SettingRow)\b[^>]*?\sname="([^"]+)"/g)].map((m) => m[1]));
/** What its Reset puts back, by name. */
const resetsOf = (part: string) => FILES[part].flatMap((f) => [...source(f).matchAll(/pref\('\w+', '([^']+)'|\{ name: '([^']+)'/g)].map((m) => m[1] ?? m[2]));
const groupsOf = (part: string) => FILES[part].flatMap((f) => [...source(f).matchAll(/<Group\b[^>]*?\stitle="([^"]+)"/g)].map((m) => m[1]));

test("the tiered parts are Personal's and This device's, and no server part", () => {
  const own = SECTIONS.filter((x) => x.kept !== 'server').map((x) => x.id);
  assert.deepEqual([...TIERED].sort(), [...own].sort());
  for (const p of ADVANCED_PARTS) assert.ok(TIERED.includes(p));
});

test('every row has a tier', () => {
  for (const part of Object.keys(FILES)) {
    const rows = rowsOf(part);
    assert.ok(rows.length > 0, part);
    for (const name of rows) assert.ok(ROWS[part as keyof typeof ROWS]?.[name], `${part}: "${name}" has no tier`);
  }
});

test('every tier a row: each names a row of its part, or what its Reset puts back', () => {
  for (const [part, tiers] of Object.entries(ROWS)) {
    const names = new Set([...rowsOf(part), ...resetsOf(part)]);
    for (const name of Object.keys(tiers ?? {})) assert.ok(names.has(name), `${part}: "${name}" is no row`);
  }
  for (const [part, tiers] of Object.entries(GROUPS)) {
    const titles = new Set(groupsOf(part));
    for (const title of Object.keys(tiers ?? {})) assert.ok(titles.has(title), `${part}: no group "${title}"`);
  }
});

test('the Advanced parts have no rows of their own here, and every other tiered part a Basic one', () => {
  for (const part of ADVANCED_PARTS) {
    assert.equal(ROWS[part], undefined, part);
    assert.equal(rowTier(part, 'anything'), 'advanced');
    assert.equal(listedInBasic(part), false);
  }
  for (const part of TIERED.filter((p) => !ADVANCED_PARTS.includes(p))) {
    assert.ok(Object.values(ROWS[part] ?? {}).includes('basic'), `${part}: nothing Basic, so it would show empty`);
    assert.equal(listedInBasic(part), true);
  }
  assert.equal(listedInBasic('connections'), true, 'a server part is left alone');
});

test("the owner's Basic: Theme, play history, levelling, where it plays, the address, the password and Spotify, the exclusions, Mobile data", () => {
  const basic: [string, string][] = [
    ['appearance', 'Theme'],
    ['appearance', 'Automatic'],
    ['tracking', 'Keep play history'],
    ['device', 'Level the volume'],
    ['device', 'Where music plays'],
    ['network', 'Public address'],
    ['account', 'Password'],
    ['account', 'Spotify'],
    ['taste', 'Everywhere'],
    ['taste', 'Artists'],
    ['taste', 'Discover'],
    ['sync', 'Mobile data'],
  ];
  for (const [part, name] of basic) assert.equal(rowTier(part as never, name), 'basic', `${part}: ${name}`);
  // And the ones the owner kept Advanced: the app password, tokens and keys, Scrobble from, adding a server.
  for (const [part, name] of [
    ['account', 'App password'],
    ['tracking', 'User token'],
    ['tracking', 'API key'],
    ['tracking', 'Scrobble from'],
    ['device', 'Add a server'],
  ])
    assert.equal(rowTier(part as never, name), 'advanced', `${part}: ${name}`);
});

test("a thing's row has no tier and shows; a server part's rows have none; a Reset lists only Basic rows' items in Basic", () => {
  assert.equal(rowTier('account', 'Phone'), null);
  assert.equal(basicShows('account', 'Phone'), true);
  assert.equal(rowTier('libraries', 'Name'), null);
  assert.equal(basicShows('sync', 'Storage'), false);
  assert.equal(basicResets('sync', 'Mobile data'), true);
  assert.equal(basicResets('sync', 'Storage'), false);
  assert.equal(basicResets('device', 'Back to start'), false, "a tape motion's item, whose row is the tape's");
  assert.equal(basicResets('diagnostics', 'More detail'), true, 'a server part resets whole');
  assert.equal(groupTier('taste', 'Profile'), 'advanced');
  assert.equal(groupTier('taste', 'Exclusions'), null);
  assert.equal(groupTier('taste', null), null);
});
