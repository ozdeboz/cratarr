// Browsing with Cratarr away, in a real browser (e2e/standin.mjs): the whole library read from what the device keeps.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, inModule, PAGE, signIn } from './browser.mjs';

const PREFS = '/src/lib/prefs.svelte.ts';
const standin = (page, what) => page.evaluate((w) => fetch(`/api/standin/${w}`).then((r) => r.json()), what);

/** A track as the page lists what it keeps (lib/offline), its stream in the cache the service worker reads. */
const keepStream = (page, who, id, album) =>
  page.evaluate(
    async ({ who, id, album }) => {
      const list = `cratarr-offline-index-2@${who}`;
      const kept = JSON.parse(localStorage.getItem(list) ?? '{}');
      kept[`cratarr/${id}`] = { source: 'cratarr', id, name: id, artist: null, album: null, album_id: album, duration_secs: 1, bytes: 1, at: 1 };
      localStorage.setItem(list, JSON.stringify(kept));
      await (await caches.open('cratarr-offline-2')).put(`/api/stream/cratarr/${id}`, new Response('sound'));
    },
    { who, id, album },
  );

/** The Library's records as they show: each one's title, dimmed or not, and whether it offers Play. */
const records = (page) =>
  page.evaluate(() =>
    Object.fromEntries(
      [...document.querySelectorAll('li[data-subject]')].map((li) => {
        const title = li.querySelector('button[aria-label^="View "]').getAttribute('aria-label').slice('View '.length);
        return [title, { off: !!li.querySelector('.off'), play: !!li.querySelector('button[aria-label^="Play "]') }];
      }),
    ),
  );

/** How wide a record's cover came out in its row: none where nothing was drawn. */
const coverDrawn = (page, title) =>
  page.evaluate((title) => {
    const row = [...document.querySelectorAll('li[data-subject]')].find((li) => li.querySelector(`button[aria-label="View ${title}"]`));
    return row?.querySelector('img')?.naturalWidth ?? 0;
  }, title);

async function until(ask, ok, what, ms = 45_000) {
  const end = Date.now() + ms;
  for (;;) {
    const now = await ask().catch((e) => e);
    if (ok(now)) return now;
    if (Date.now() > end) assert.fail(`${what}: ${JSON.stringify(now)}`);
    await new Promise((go) => setTimeout(go, 250));
  }
}

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

test('Cratarr gone, what is not here is dimmed, Available offline shows what plays, and a stream the browser dropped reads as not here', async () => {
  const page = d.page;
  await signIn(page, 'ann');
  // Where the stand-in's library stands, which index.e2e moves: the index and its covers in step.
  await until(() => inModule(page, '/src/lib/deviceSync.svelte.ts', (m) => [m.indexSync.rev, m.indexSync.covers]), ([index, covers]) => index !== null && index === covers, 'the index', 60_000);
  await keepStream(page, 'ann', 'tr-kept', 'al-10');
  await page.goto(`${PAGE}/#/library`);
  await page.reload();
  await standin(page, 'down');
  await until(() => records(page), (r) => r['Second Wind']?.off && r['First Light']?.off === false, 'the marks');
  assert.deepEqual((await records(page))['Second Wind'], { off: true, play: false });
  assert.deepEqual((await records(page))['First Light'], { off: false, play: true });
  await until(() => coverDrawn(page, 'Second Wind'), (w) => w > 0, 'the small cover of a record not here');

  const was = await inModule(page, PREFS, (m) => m.prefs.albumLayout);
  const shown = Object.keys(await records(page)).length;
  await inModule(page, PREFS, (m) => void (m.prefs.albumLayout = 'grid'));
  await until(() => page.locator('.tile.off').count(), (n) => n === shown - 1, 'the tiles: all but the one with something here');
  assert.deepEqual((await records(page))['First Light'], { off: false, play: true });
  await inModule(page, PREFS, (m, was) => void (m.prefs.albumLayout = was), was);

  await page.locator('button.funnel').first().click();
  await page.getByText('Available offline').click();
  await page.keyboard.press('Escape');
  await until(() => records(page), (r) => r['First Light'] && !r['Second Wind'], 'what plays alone');
  const said = await page.locator('[role=alert], .toast').allInnerTexts();
  assert.deepEqual(said, [], 'nothing said of what the filter could not ask');

  await page.evaluate(() => caches.open('cratarr-offline-2').then((c) => c.delete('/api/stream/cratarr/tr-kept')));
  await page.reload();
  await until(() => inModule(page, '/src/lib/offline.svelte.ts', (m) => Object.keys(m.offline.kept).length), (n) => n === 0, 'the list held to the cache');
  await inModule(page, '/src/lib/listFilter.svelte.ts', (m) => m.setSwitch('library', 'here', false));
  await until(() => records(page), (r) => r['First Light']?.off && r['Second Wind']?.off, 'a dropped stream reads as not here');
});

test('Cratarr gone, search finds the library in what the device keeps, says nothing of the rest, and an artist found opens', async () => {
  const page = d.page;
  await page.locator('button.lens').first().click();
  // A track at every rev the stand-in's library stands at.
  await page.locator('input[type=search]').fill('noon');
  await page.locator('button.globe').click({ timeout: 20_000 });
  await page.getByRole('button', { name: 'Open Noon' }).waitFor({ timeout: 20_000 });
  const everywhere = await page.locator('section[aria-label="Everywhere"]').innerText();
  assert.ok(!everywhere.includes('Cannot reach'), everywhere);
  assert.ok(!/Playlists|Crates|Genres|Requested/.test(everywhere), everywhere);

  await page.locator('input[type=search]').fill('ana bell');
  await page.getByRole('button', { name: 'Open Ana Bell' }).first().click({ timeout: 20_000 });
  await page.getByText(/^\d+ albums • \d+ tracks/).first().waitFor({ timeout: 20_000 });
  assert.deepEqual(await page.locator('[role=alert], .toast').allInnerTexts(), [], 'nothing said of what could not be asked');
});
