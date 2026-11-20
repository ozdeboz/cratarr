// What Settings reaches, for whom (docs/ui-conventions.md, the parts): a link to a part goes to the part that shows the thing, and
// someone who cannot open that part is told who can. The guest page can be switched on with no host set. A device's own rows are
// its owner's, administrator or not. These read the source: each is one line of a page that needs a whole app to run.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
(globalThis as Record<string, unknown>).$effect = () => {};
const { partsFor } = await import('../src/lib/settingsSections.svelte.ts');

const source = (file: string) => readFileSync(new URL(`../src/${file}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const ids = (admin: boolean) => partsFor(admin, false, false).map((p: { id: string }) => p.id);

test('someone not an administrator reaches Devices, and nothing else of the server', () => {
  assert.deepEqual(ids(false).filter((id: string) => ['connections', 'libraries', 'players', 'users', 'auth', 'serverimport', 'diagnostics'].includes(id)), ['players']);
  assert.ok(ids(true).includes('players'));
});

test('Devices shows them their own devices: what is the server\'s stays the administrators\'', () => {
  const page = source('lib/PlayersSettings.svelte');
  assert.ok(page.includes('{#if me.admin}<KodiSettings'), 'a Kodi is the server\'s');
  assert.ok(page.includes('{#if !thing && me.admin && runtime.value}'), 'so is Playing in step\'s address');
  assert.ok(page.includes('if (me.admin) loadRuntime();'), 'and it is not asked for');
  assert.ok(page.includes('const own = $derived(playOn.players.filter((p) => p.mine !== false));'), 'the list is their own');
});

test('every place that says who sees a part lets a part for everyone through: one rule, read by each (CU23)', () => {
  assert.ok(source('lib/settingsSections.svelte.ts').includes('(!x.admin || admin || x.everyone)'));
  for (const file of ['App.svelte', 'Settings.svelte', 'lib/SettingsPanel.svelte', 'lib/everywhere.svelte.ts']) {
    assert.ok(source(file).includes('partsShown(me.admin, inApp, installed, '), file);
    assert.ok(!source(file).includes('(!x.admin || me.admin || x.everyone)'), `${file}: no rule of its own`);
  }
  // Opened by its address a part is not narrowed: whether it may show at all stays the permission alone.
  assert.ok(source('lib/SettingsPanel.svelte').includes('return !x?.admin || me.admin || !!x.everyone;'));
});

test('Discover sends one to Tracking, where the listening services are', () => {
  const page = source('Discover.svelte');
  assert.ok(page.includes("<a href={viewOn('settings', 'settings:tracking')}>connect one</a>"));
  assert.ok(!page.includes("viewOn('settings', 'settings:connections')"), 'not to Connections, blank for anyone but an administrator');
});

test('a link into Connections shows to administrators alone: anyone else is told who can', () => {
  for (const file of ['Discover.svelte', 'lib/RequestsPanel.svelte']) {
    const page = source(file);
    const links = [...page.matchAll(/.*<a href=\{viewOn\('settings', 'settings:connections[^']*'\)\}>.*/g)].map((m) => m[0]);
    assert.ok(links.length > 0, file);
    for (const line of links) assert.match(line, /\{#if me\.admin\}<a href=.*\{:else\}an administrator can connect/, line.trim());
  }
});

test('the words say Tracking where the listening services went', () => {
  assert.ok(source('lib/settings/ImportPart.svelte').includes('connected under Tracking'));
});

test('the guest page can be switched on with no host: the switch asks for one, and the choice switches it on', () => {
  const page = source('lib/settings/AuthPart.svelte');
  assert.ok(page.includes('if (on && !g.host) {'), 'a switch with no host asks');
  assert.ok(page.includes('on: g.on || hostToSwitchOn'), 'the host chosen switches it on');
  assert.ok(!page.includes('{#if g.on}<Item name="Guests ask"'), 'and the row is there while it is off');
});

test('Quality\'s default is one in the page and the app: smaller copies', () => {
  const app = readFileSync(new URL('../../android/app/src/main/java/app/cratarr/Prefs.kt', import.meta.url), 'utf8');
  assert.ok(app.includes('p.getString(DOWNLOAD_QUALITY, QUALITY_SMALLER)'));
  assert.ok(source('lib/prefs.svelte.ts').includes("offlineQuality: 'smaller',"));
  assert.ok(source('lib/settings/SyncPart.svelte').includes("now: () => appInfo.downloadQuality, def: 'smaller'"), 'Reset puts back what a new install has');
});

test('Scrobble from\'s default is one in the server, the page and the app: from each device (0091)', () => {
  const app = readFileSync(new URL('../../android/app/src/main/java/app/cratarr/Scrobblers.kt', import.meta.url), 'utf8');
  assert.ok(app.includes('getString(Who.keyed(context, WAY), "device")'));
  assert.ok(source('lib/settings/TrackingPart.svelte').includes("value={scrobbling.way ?? 'device'}"));
  assert.ok(source('lib/scrobbles.svelte.ts').includes("r.way === 'server' ? 'server' : 'device'"));
});
