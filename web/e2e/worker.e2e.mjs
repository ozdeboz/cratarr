// The service worker's copies per account (decision 0101, point 14; public/sw.js): each account's kept reads, its
// album list among them, apart from the others'; none kept for nobody.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, signIn, signOut } from './browser.mjs';

/** The page under the worker's control, and the account's reads made through it. */
async function readsThroughWorker(page) {
  await page.waitForFunction(() => navigator.serviceWorker?.controller, null, { timeout: 20_000 }).catch(() => {});
  if (!(await page.evaluate(() => !!navigator.serviceWorker?.controller))) {
    await page.reload();
    await page.waitForFunction(() => navigator.serviceWorker?.controller, null, { timeout: 20_000 });
  }
  await page.evaluate(async () => {
    for (const p of ['/api/me', '/api/albums', '/api/playlists']) await fetch(p).catch(() => {});
    await new Promise((r) => setTimeout(r, 1500));
  });
}
const cacheNames = (page) => page.evaluate(() => caches.keys());
const keptName = (page, cacheName) =>
  page.evaluate(async (cacheName) => {
    const hit = await caches.match('/api/me', { cacheName });
    return hit ? (await hit.json()).name : null;
  }, cacheName);

let d;
before(async () => (d = await device()));
after(async () => {
  assert.deepEqual(d.errors, [], 'the page threw nothing');
  await d.browser.close();
});

test('an account’s reads are kept as its own, the album list among them', async () => {
  await signIn(d.page, 'ann');
  await readsThroughWorker(d.page);
  assert.ok((await cacheNames(d.page)).includes('cratarr-data-2@ann'));
  assert.equal(await keptName(d.page, 'cratarr-data-2@ann'), 'ann');
  assert.ok(await d.page.evaluate(async () => !!(await caches.match('/api/albums', { cacheName: 'cratarr-data-2@ann' }))));
  assert.ok(await d.page.evaluate(async () => !(await caches.match('/api/albums', { cacheName: 'cratarr-v5' }))), 'not the app’s own cache');
});

test('signed out, the worker keeps for nobody', async () => {
  await signOut(d.page);
  const who = await d.page.evaluate(async () => (await (await caches.open('cratarr-who')).match('/__who'))?.text());
  assert.equal(who, '-');
});

test('another account’s are its own, hers kept apart', async () => {
  await signIn(d.page, 'bob');
  await readsThroughWorker(d.page);
  const names = await cacheNames(d.page);
  assert.ok(names.includes('cratarr-data-2@bob') && names.includes('cratarr-data-2@ann'));
  assert.equal(await keptName(d.page, 'cratarr-data-2@bob'), 'bob');
  assert.equal(await keptName(d.page, 'cratarr-data-2@ann'), 'ann');
});
