// What a device does between two accounts when no one signs out by the book (decision 0101, points 13 and 14): a session
// that ended with no page open, another tab, and the change of account itself, which only a browser's own reactions can
// show to be no change of anyone's preferences.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, inModule, PAGE, signIn, storage } from './browser.mjs';

const PREFS = '/src/lib/prefs.svelte.ts';
const ACCOUNT = '/src/lib/account.ts';
const settled = () => new Promise((r) => setTimeout(r, 1200));
/** When each of `who`'s preferences last changed here, as this device's copy says. */
const changed = async (page, who) => JSON.parse((await storage(page))[`cratarr-prefs-account@${who}`] ?? '{}').changed ?? {};

let d;
before(async () => (d = await device()));
after(async () => {
  assert.deepEqual(d.errors, [], 'the page threw nothing');
  await d.browser.close();
});

test('another one signing in is no change of anyone’s: nothing is stamped as made now', async () => {
  await signIn(d.page, 'ann');
  await inModule(d.page, PREFS, (m) => void (m.prefs.pinned = ['pl-ann']));
  await settled();
  const hers = await changed(d.page, 'ann');
  assert.ok(hers.pinned, 'her change noted, with when');
  // In the page, with its reactions running: as a tab sees it, nobody and then her again.
  await inModule(d.page, ACCOUNT, (m) => m.accountForgotten());
  await settled();
  assert.deepEqual(await inModule(d.page, PREFS, (m) => [...m.prefs.pinned]), [], 'nobody has none of hers');
  assert.equal((await changed(d.page, 'ann')).pinned, hers.pinned, 'and her copy says what it said');
  await inModule(d.page, ACCOUNT, (m) => m.accountLearnt('ann'));
  await settled();
  assert.deepEqual(await inModule(d.page, PREFS, (m) => [...m.prefs.pinned]), ['pl-ann']);
  assert.equal((await changed(d.page, 'ann')).pinned, hers.pinned, 'hers again is no change of hers either');
});

test('a crate being made and Home’s picture are hers here, under her name', async () => {
  await inModule(d.page, '/src/lib/crates.svelte.ts', (m) => m.keepDraft(null, { name: 'Her crate', conditions: [] }));
  await settled();
  const s = await storage(d.page);
  assert.equal(JSON.parse(s['cratarr-prefs@ann']).crateDrafts.new.name, 'Her crate');
  assert.ok(!('crateDrafts' in JSON.parse(s['cratarr-prefs'])), 'none of it among the device’s');
});

test('a session that ended with no page open: whoever signs in next opens on nothing of hers, not for a moment', async () => {
  await d.page.goto('about:blank');
  await d.page.context().clearCookies();
  // Every store the next page opens or reads, from before its first line.
  await d.page.addInitScript(() => {
    const read = (window.__read = []);
    const get = Storage.prototype.getItem;
    Storage.prototype.getItem = function (k) {
      read.push(String(k));
      return get.call(this, k);
    };
    const open = IDBFactory.prototype.open;
    IDBFactory.prototype.open = function (name, ...rest) {
      read.push(String(name));
      return open.call(this, name, ...rest);
    };
  });
  await signIn(d.page, 'bob', '/signin?next=%2F');
  const read = await d.page.evaluate(() => window.__read);
  assert.ok(read.some((k) => k.endsWith('@bob') || k === 'cratarr-who'), 'the watch was on');
  assert.deepEqual([...new Set(read.filter((k) => k.endsWith('@ann')))], []);
});

test('what the worker kept of hers stayed hers, and his reads are his', async () => {
  await d.page.evaluate(async () => {
    await fetch('/api/me');
    await new Promise((r) => setTimeout(r, 800));
  });
  const name = (cacheName) => d.page.evaluate(async (cacheName) => (await (await caches.match('/api/me', { cacheName }))?.json())?.name ?? null, cacheName);
  assert.equal(await name('cratarr-data-2@bob'), 'bob');
  assert.equal(await name('cratarr-data-2@ann'), null, 'ann read none through it here, and none of his went there');
});

test('another account in one tab starts another over at once: nothing is refused it, and what it holds in memory is the one before’s', async () => {
  const other = await d.page.context().newPage();
  await other.goto(`${PAGE}/`);
  await other.waitForFunction(() => localStorage.getItem('cratarr-who') === '"bob"');
  await other.evaluate(() => (window.__before = true));
  await d.page.evaluate(() => localStorage.setItem('cratarr-who', '"ann"'));
  await other.waitForFunction(() => !window.__before, null, { timeout: 5_000 });
  await other.close();
});
