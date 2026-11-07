// The installed page's own kept list, in a real browser (e2e/standin.mjs; the owner, 2026-11-07): one this browser cannot read
// says so, and starts again from the streams the browser holds, which stay.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, PAGE, signIn } from './browser.mjs';

const LIST = 'cratarr-offline-index-2@ann';
const STREAMS = ['/api/stream/cratarr/tr-kept-1', '/api/stream/cratarr/tr-kept-2'];

let d;
before(async () => (d = await device()));
after(() => d.browser.close());

test('a kept list this browser cannot read says so, and starts again from the streams it holds, which stay', async () => {
  // Installed, as a home screen opens it: the page keeps tracks itself (lib/offline).
  const context = await d.browser.newContext();
  await context.addInitScript(() => Object.defineProperty(Navigator.prototype, 'standalone', { get: () => true }));
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await signIn(page, 'ann');
  // Two streams kept and their list broken, on an address of Cratarr's where no page runs to write it meanwhile.
  await page.goto(`${PAGE}/api/standin/up`);
  await page.evaluate(
    async ({ list, streams }) => {
      const cache = await caches.open('cratarr-offline-2');
      for (const s of streams) await cache.put(s, new Response('sound'));
      localStorage.setItem(list, '{"cratarr/tr-kept-1": {"sou');
    },
    { list: LIST, streams: STREAMS },
  );
  await page.goto(`${PAGE}/#/downloads`);
  const said = page.getByText("Kept list can't be read");
  await said.waitFor({ timeout: 45_000 });

  // Asked first, then started again.
  await page.getByRole('button', { name: 'Start again' }).click();
  const sheet = page.getByRole('dialog', { name: 'Start the kept list again?' });
  await sheet.getByText('Downloads stay.').waitFor();
  await sheet.getByRole('menuitem', { name: 'Start again' }).click();
  for (const id of ['tr-kept-1', 'tr-kept-2']) await page.getByRole('button', { name: `Remove ${id} from this browser` }).waitFor({ timeout: 20_000 });
  assert.equal(await said.count(), 0);
  const { list, held } = await page.evaluate(async (list) => {
    const keys = await (await caches.open('cratarr-offline-2')).keys();
    return { list: localStorage.getItem(list), held: keys.map((r) => new URL(r.url).pathname).sort() };
  }, LIST);
  assert.deepEqual(Object.keys(JSON.parse(list)).sort(), ['cratarr/tr-kept-1', 'cratarr/tr-kept-2'], 'written down');
  assert.deepEqual(held, STREAMS, 'the streams stayed');
  assert.deepEqual(errors, [], 'the page threw nothing');
  await context.close();
});
