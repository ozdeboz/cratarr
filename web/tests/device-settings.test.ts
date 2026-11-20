// This device's settings, one owner each (lib/deviceSettings; the architecture audit, 2026-10-29): the registry, and
// that nothing reads a second copy: Start and stop and levelling come from the owner's copy through one way only.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const { DEVICE_SETTINGS, ownCopy, ownerOf } = await import('../src/lib/deviceSettings.ts');
const SRC = new URL('../src/', import.meta.url).pathname.replace(/^\/(\w:)/, '$1');
const CODE = /\.(ts|svelte)$/;
function files(dir: string): string[] {
  const out: string[] = [];
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) out.push(...files(p));
    else if (CODE.test(f)) out.push(p);
  }
  return out;
}
const rel = (p: string) => p.slice(SRC.length).split(String.fromCharCode(92)).join('/');

test('each setting once, with an owner on each kind of device it exists on', () => {
  const ids = DEVICE_SETTINGS.map((s) => s.id);
  assert.equal(new Set(ids).size, ids.length);
  assert.equal(ownerOf('startStop', true), 'app');
  assert.equal(ownerOf('startStop', false), 'page');
  assert.equal(ownerOf('delay', true), 'server', "a speaker delay is Cratarr's, the app's or a browser's player alike");
  assert.equal(ownerOf('stayReady', false), null, 'a browser has none');
  assert.equal(ownCopy(true, 'app', 'page'), 'app');
  assert.equal(ownCopy(false, 'app', 'page'), 'page');
});

test("what the app keeps is what setDevice sends it", () => {
  const app = readFileSync(join(SRC, 'lib/app.svelte.ts'), 'utf8');
  const sent = app.slice(app.indexOf('export const setDevice'), app.indexOf("toApp({ type: 'device-set'"));
  for (const s of DEVICE_SETTINGS.filter((x) => x.app === 'app'))
    for (const k of s.appKeys ?? []) assert.ok(sent.includes(` ${k}?:`), `${s.id}: setDevice has no ${k}`);
});

test('no second copy: Start and stop and levelling read and changed through their one way', () => {
  const forks = /inApp \? appInfo\.(startStop|levelling|levelUnknown)\b/;
  const writes = /\b(appInfo|prefs)\.(startStop|levelling|levelUnknown) = /;
  // Who may write a copy: the app's answer (lib/app), the one way (lib/device, lib/level), and this browser's own
  // player where a command or its switch sets this browser's (lib/localPlayer).
  const writers = new Set(['lib/app.svelte.ts', 'lib/device.svelte.ts', 'lib/level.svelte.ts', 'lib/localPlayer.svelte.ts']);
  for (const f of files(SRC)) {
    const src = readFileSync(f, 'utf8');
    assert.ok(!forks.test(src), `${rel(f)}: picks between the app's copy and this browser's itself`);
    if (!writers.has(rel(f))) assert.ok(!writes.test(src), `${rel(f)}: writes a copy past its owner`);
  }
});

// CU13 (Q4 (a); red first: Sync had a "Download over mobile data" switch): one "Mobile data" choice in Sync, the app's, of three:
// All, Not downloads (as before, the default) and None; it replaces the switch everywhere the page has it.
test("Sync's Mobile data: one choice of three in place of the download switch, the app's to keep", () => {
  const sync = readFileSync(join(SRC, 'lib/settings/SyncPart.svelte'), 'utf8');
  assert.match(sync, /<ChoiceRow\s+name="Mobile data"[^>]*options=\{MOBILE\}\s+value=\{appInfo\.mobileData\}\s+onchange=\{\(v\) => setDevice\(\{ mobileData: v \}\)\}/);
  assert.match(sync, /\{ value: 'all', label: 'All' \},\s+\{ value: 'notDownloads', label: 'Not downloads' \},\s+\{ value: 'none', label: 'None' \},/);
  assert.ok(sync.includes("{ name: 'Mobile data', now: () => appInfo.mobileData, def: 'notDownloads',"), 'its Reset puts back Not downloads');
  assert.equal(DEVICE_SETTINGS.find((s) => s.id === 'sync')?.appKeys?.includes('mobileData'), true);
  for (const f of files(SRC)) assert.ok(!/downloadOverMobile|Download over mobile data/.test(readFileSync(f, 'utf8')), `${rel(f)}: the switch it replaces`);
  const app = readFileSync(join(SRC, 'lib/app.svelte.ts'), 'utf8');
  assert.ok(app.includes("appInfo.mobileData = m.mobileData === 'all' ? 'all' : m.mobileData === 'none' ? 'none' : 'notDownloads';"));
});
