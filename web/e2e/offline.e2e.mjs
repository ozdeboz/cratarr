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

test("a list's covers are asked at the size they are drawn, never whole, and a record's panel asks its own whole", async () => {
  // A browser of its own, so that every cover is asked as it comes, none from what an earlier test left; two device pixels to a
  // CSS one, as a tablet's.
  await standin(d.page, 'up');
  const context = await d.browser.newContext({ deviceScaleFactor: 2 });
  const page = await context.newPage();
  await signIn(page, 'ann');
  await until(() => inModule(page, '/src/lib/deviceSync.svelte.ts', (m) => [m.indexSync.rev, m.indexSync.covers]), ([index, covers]) => index !== null && index === covers, 'the index', 60_000);
  await standin(page, 'down');
  const asked = [];
  page.on('request', (r) => {
    const u = new URL(r.url());
    if (/^\/api\/albums\/[^/]+\/[^/]+\/cover$/.test(u.pathname)) asked.push(u.pathname + u.search);
  });
  /** Each cover `selector` finds: the device pixels its box holds, and the size it asked. */
  const drawn = (selector) =>
    page.evaluate(
      (selector) =>
        [...document.querySelectorAll(selector)].map((img) => {
          const box = img.getBoundingClientRect();
          return { px: Math.max(box.width, box.height) * devicePixelRatio, size: Number(new URL(img.src).searchParams.get('size')) };
        }),
      selector,
    );
  /** The least size that fills it. */
  const fits = (r) => r.size >= r.px && (r.size === 160 || r.size / 2 < r.px);
  const layout = (to) => inModule(page, PREFS, (m, to) => void (m.prefs.albumLayout = to), to);

  // Rows, their covers lazy.
  await layout('rows');
  await page.goto(`${PAGE}/#/library`);
  await page.locator('li[data-subject] img').first().waitFor({ timeout: 45_000 });
  for (const r of await drawn('li[data-subject] img')) assert.ok(fits(r), JSON.stringify(r));
  // A grid's tiles, large enough that the device's pixels and CSS's ask different sizes.
  await layout('grid');
  await until(() => drawn('.tile img'), (rs) => rs.length > 0 && rs.every(fits), 'the tiles at the size they are drawn');
  // The flow draws its covers at once, not lazily: nothing whole even so.
  await layout('flow');
  await until(() => page.evaluate(() => document.querySelectorAll('.lists img').length), (n) => n > 0, 'the flow');
  await page.waitForTimeout(1_000);
  assert.ok(asked.length > 0 && asked.every((a) => a.includes('size=')), `nothing whole, not even before a box is laid out: ${JSON.stringify(asked)}`);

  // A record's panel: its own cover whole, beside the list, and alone in its flow in a short, wide window, as a phone on its side.
  await layout('rows');
  await page.getByRole('button', { name: 'View First Light' }).first().click();
  const own = () => page.evaluate(() => [...document.querySelectorAll('.hero img')].map((img) => img.getAttribute('src')));
  await until(own, (srcs) => srcs.length > 0 && srcs.every((src) => !src.includes('size=')), "the panel's own cover, whole");
  await page.setViewportSize({ width: 900, height: 500 });
  await until(() => page.evaluate(() => !!document.querySelector('.hero.flowing img')), (alone) => alone, 'its cover alone in its flow');
  await until(own, (srcs) => srcs.length > 0 && srcs.every((src) => !src.includes('size=')), "the panel's own cover in its flow, whole");
  await standin(page, 'up');
  await context.close();
});
