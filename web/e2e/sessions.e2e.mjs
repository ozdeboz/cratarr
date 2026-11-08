// What a device does between two accounts when no one signs out by the book (decision 0101, points 13 and 14): a session
// that ended with no page open, another tab, and the change of account itself, which only a browser's own reactions can
// show to be no change of anyone's preferences.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { askSignOut, device, inModule, PAGE, signIn, signInOnly, signOut, storage } from './browser.mjs';

const PREFS = '/src/lib/prefs.svelte.ts';
const ACCOUNT = '/src/lib/account.ts';
const settled = () => new Promise((r) => setTimeout(r, 1200));
/** When each of `who`'s preferences last changed here, as this device's copy says. */
const changed = async (page, who) => JSON.parse((await storage(page))[`cratarr-prefs-account@${who}`] ?? '{}').changed ?? {};

let d;
before(async () => (d = await device()));
after(async () => {
  // Here reads are cut on purpose, and a read with nothing to catch it says so.
  assert.deepEqual(d.errors.filter((e) => !e.includes('Cannot reach')), [], 'the page threw nothing');
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
  await inModule(d.page, '/src/lib/localCovers.svelte.ts', (m) => m.takeHomePicture('data:image/gif;base64,R0lGODlhAQABAAAAACw='));
  await settled();
  const s = await storage(d.page);
  assert.equal(JSON.parse(s['cratarr-prefs@ann']).crateDrafts.new.name, 'Her crate');
  assert.ok(!('crateDrafts' in JSON.parse(s['cratarr-prefs'])), 'none of it among the device’s');
  assert.ok(s['cratarr-home-picture@ann'], 'the copy for her account');
  assert.ok(!('cratarr-home-picture' in s), 'none under a name for no one');
  const databases = await d.page.evaluate(async () => (await indexedDB.databases()).map((b) => b.name));
  assert.ok(databases.includes('cratarr-covers@ann'), 'the picture itself, in her own database');
  assert.ok(!databases.includes('cratarr-covers'));
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

test('his reads are kept as his, and none of them lands among hers', async () => {
  await d.page.evaluate(async () => {
    await fetch('/api/me');
    await new Promise((r) => setTimeout(r, 800));
  });
  const name = (cacheName) => d.page.evaluate(async (cacheName) => (await (await caches.match('/api/me', { cacheName }))?.json())?.name ?? null, cacheName);
  assert.equal(await name('cratarr-data-2@bob'), 'bob');
  assert.equal(await name('cratarr-data-2@ann'), null);
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

test('Cratarr’s sign-in page is nobody’s: reached with the session over, the name kept goes, signed in or not', async () => {
  await signOut(d.page);
  await signIn(d.page, 'ann', '/signin');
  await d.page.goto('about:blank');
  await d.page.context().clearCookies();
  await d.page.goto(`${PAGE}/signin`);
  assert.equal((await storage(d.page))['cratarr-who'], 'null');
});

test('signed in, Cratarr not reached again to say who: a reload is no one’s, not the name of whoever was here before', async () => {
  // With no worker: a page's route does not see what a worker asks, and it is /api/me that has to go unanswered.
  const context = await d.browser.newContext({ serviceWorkers: 'block' });
  const page = await context.newPage();
  await signIn(page, 'ann', '/signin');
  // Off the page first, as the tests beside it go: left open, its next request meets the session gone, and it sends itself to the
  // sign-in page (lib/reach's toSignIn) while the sign-in below is on its way there, which then never arrives.
  await page.goto('about:blank');
  await context.clearCookies();
  await page.route('**/api/me', (r) => r.abort());
  await signInOnly(page, 'bob', '/signin');
  await page.reload();
  assert.equal(await inModule(page, ACCOUNT, (m) => m.accountKey()), null);
  await context.close();
});

test('a sign-out that did not reach Cratarr: the next open is not hers, and the session is ended before anything else', async () => {
  await signIn(d.page, 'ann', '/signin');
  await d.page.route('**/signout', (r) => r.abort());
  await askSignOut(d.page);
  // Through the page's own move to itself, as nobody.
  await d.page.waitForFunction(() => localStorage.getItem('cratarr-sign-out-owed') !== null && localStorage.getItem('cratarr-who') === 'null');
  await d.page.waitForLoadState('load');
  await d.page.unroute('**/signout');
  // Open again, Cratarr back and her session standing: the page must not take it up, and ends it.
  await d.page.goto(`${PAGE}/`);
  await d.page.waitForURL(/\/signin/, { timeout: 20_000 });
  const s = await storage(d.page);
  assert.equal(s['cratarr-who'], 'null');
  assert.equal(s['cratarr-sign-out-owed'], undefined);
  const me = await d.page.evaluate(async () => (await fetch('/api/me')).status);
  assert.equal(me, 401, 'and it is over');
});

test('owed, its session then ended some other way: the sign-in page settles it, or the next one in would be signed out for it', async () => {
  await signIn(d.page, 'ann', '/signin');
  await d.page.route('**/signout', (r) => r.abort());
  await askSignOut(d.page);
  await d.page.waitForFunction(() => localStorage.getItem('cratarr-sign-out-owed') !== null);
  await d.page.waitForLoadState('load');
  await d.page.unroute('**/signout');
  await d.page.goto('about:blank');
  await d.page.context().clearCookies();
  await signIn(d.page, 'bob', '/signin');
  await d.page.reload();
  await d.page.waitForLoadState('load');
  assert.equal(await d.page.evaluate(async () => (await fetch('/api/me')).status), 200);
  await signOut(d.page);
});

test('a sign-out that reached Cratarr ends the session it held', async () => {
  await signIn(d.page, 'ann', '/signin');
  await signOut(d.page);
  assert.equal(await d.page.evaluate(async () => (await fetch('/api/me')).status), 401);
});
