// Browsing with Cratarr away, in a real browser (e2e/standin.mjs): the whole library read from what the device keeps.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, PAGE, signIn } from './browser.mjs';

const standin = (page, what) => page.evaluate((w) => fetch(`/api/standin/${w}`).then((r) => r.json()), what);

let d;
before(async () => (d = await device()));
after(async () => {
  await standin(d.page, 'up').catch(() => {});
  await d.browser.close();
  assert.deepEqual(d.errors, [], 'the page threw nothing');
});

test('a page opened with Cratarr gone and no service worker to answer throws nothing', async () => {
  const context = await d.browser.newContext({ serviceWorkers: 'block' });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await signIn(page, 'ann');
  await standin(page, 'down');
  await page.goto(PAGE);
  await page.waitForTimeout(3_000);
  await standin(page, 'up');
  await context.close();
  assert.deepEqual(errors, []);
});
