// The search field's words and its fold, as the owner gave the rules (2026-11-25): Home always empties and folds it, Home
// itself too; Esc empties and folds it; another page by the side menu starts it empty; a playlist opened from a search
// takes the words away, and one found deep (by its tracks) opens with those tracks marked instead; only Back onto the step
// they were saved on brings them back, never the panel's own step. The playlists, a crate and the search's answer are given
// here (the stand-in has none).
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, inModule, PAGE, signIn } from './browser.mjs';

const VIEW = '/src/lib/pageView.svelte.ts';
const words = (page) => inModule(page, VIEW, (m) => m.pageView.query);
const open = (page) => page.locator('span.field[role=search] input').count().then((n) => n > 0);

async function typeIn(page, text) {
  await page.locator('button.lens').first().click();
  const field = page.locator('span.field[role=search] input');
  await field.waitFor({ timeout: 10_000 });
  await field.fill(text);
  assert.equal(await words(page), text);
}

/** Until `check` holds, or the time is up (the page empties the search when the page gone to shows). */
async function until(check, what) {
  for (let i = 0; i < 40; i++) {
    if (await check()) return;
    await new Promise((go) => setTimeout(go, 100));
  }
  assert.fail(what);
}

const NIGHT = { id: 'Night drives', name: 'Night drives', member: true, count: 2, dead: 0, links: [], spoken: [], owner: 'ann', shared: false, own: true };
const row = (id, name, artist, album) => ({ entry: `e-${id}`, entry_id: `e-${id}`, file: `${id}.flac`, id, name, artist, album, duration_secs: 200, missing: false });
const ROWS = [row('t1', 'Highway', 'Kite Theory', 'Lamplight'), row('t2', 'Moon river', 'Glass Harbour', 'Night Walks')];
const CRATE = { id: 'deep-focus', name: 'Deep focus', favourites: 'all', favourite_albums: 'all', counts: { all: 2, only: 0, without: 2 }, permanent: false, image: false };

/** Context routes, not the page's: some reads go through the page's service worker, which the page's routes never see. */
/** What the stand-in does not have: two playlists' worth of state, one's rows, a crate, and the server's search finding the
 *  playlist by a track. */
async function fixtures(page) {
  const json = (body) => (r) => r.fulfill({ json: body });
  await page.context().route(/\/api\/state(\?|$)/, json({ now_playing: null, playlists: [NIGHT], sources: [] }));
  await page.context().route(/\/api\/playlists\/Night%20drives\/rows/, json({ source: 'cratarr', rows: ROWS }));
  await page.context().route(/\/api\/crates(\?|$)/, json([CRATE]));
  await page.context().route(/\/api\/search\?/, (r) => {
    const q = new URL(r.request().url()).searchParams.get('q') ?? '';
    const deep = /moon/i.test(q)
      ? [{ kind: 'playlist', subject: 'playlist:Night drives', title: 'Night drives', matched: [{ on: 'track', text: 'Moon river', count: 1 }] }]
      : [];
    return r.fulfill({ json: { q, groups: { playlist: { total: deep.length, items: deep } }, top: deep } });
  });
}

/** `hash` at a desktop's width, no panel open, the search empty. */
async function at(page, hash, size = { width: 1400, height: 900 }) {
  await page.setViewportSize(size);
  await page.goto(`${PAGE}/${hash}`);
  await inModule(page, '/src/lib/panel.svelte.ts', (m) => m.closePlaylist());
  await inModule(page, VIEW, (m) => (m.pageView.query = ''));
}

const sideLink = (page, label) => page.locator('nav[aria-label="Pages"] a', { hasText: label }).first();

let d;
before(async () => {
  d = await device();
  await d.page.setViewportSize({ width: 1400, height: 900 });
  await fixtures(d.page);
  await signIn(d.page, 'ann');
});
after(async () => {
  assert.deepEqual(d.errors, [], 'the page threw nothing');
  await d.browser.close();
});

test('another page by the side menu starts with its search empty and folded (Library to Playlists kept the words)', async () => {
  await at(d.page, '#/library');
  await typeIn(d.page, 'air');
  await sideLink(d.page, 'Playlists').click();
  await d.page.waitForURL(/#\/playlists/);
  await until(async () => (await words(d.page)) === '', 'the words stayed on Playlists');
  assert.equal(await open(d.page), false, 'the field folded');
  assert.equal(await d.page.locator('button.lens.lit').count(), 0, 'the magnifier not lit');
});

test('Home on Home empties and folds the search', async () => {
  await at(d.page, '#/home');
  await typeIn(d.page, 'moon');
  await sideLink(d.page, 'Home').click();
  await until(async () => (await words(d.page)) === '', 'Home on Home kept the words');
  assert.equal(await open(d.page), false, 'the field folded');
});

test('Esc empties and folds the search', async () => {
  await at(d.page, '#/library');
  await typeIn(d.page, 'talkie');
  await d.page.locator('span.field[role=search] input').press('Escape');
  await until(async () => (await words(d.page)) === '', 'Esc kept the words');
  assert.equal(await open(d.page), false, 'the field folded');
});

test("the panel's own step carries no saved search: only Back onto the step it was saved on can bring it back", async () => {
  // A phone's width: there a panel over the page takes a step of its own (beside the page it takes none).
  await at(d.page, '#/library', { width: 390, height: 844 });
  await typeIn(d.page, 'night');
  // As a tapped result leaves it (lib/globalSearch's rememberSearch), then a panel opened over the page.
  await d.page.evaluate(() => history.replaceState({ ...(history.state ?? {}), search: { q: 'night', scope: 'page' } }, ''));
  await inModule(d.page, '/src/lib/panel.svelte.ts', (m) => m.viewPlaylist('Night drives'));
  await until(() => d.page.evaluate(() => !!history.state?.panelGuard), 'no panel step');
  assert.equal(await d.page.evaluate(() => history.state?.search ?? null), null, "the panel's step holds no search");
});

test('a playlist opened from a search takes the words away and folds the field', async () => {
  await at(d.page, '#/playlists');
  await typeIn(d.page, 'night');
  await d.page.getByLabel('View Night drives').first().click();
  await until(async () => (await words(d.page)) === '', 'the words stayed with the playlist open');
  assert.equal(await open(d.page), false, 'the field folded');
});

test('a crate opened from a search takes the words away and folds the field', async () => {
  await at(d.page, '#/crates');
  await typeIn(d.page, 'focus');
  await d.page.getByLabel('View Deep focus').first().click();
  await until(async () => (await words(d.page)) === '', 'the words stayed with the crate open');
  assert.equal(await open(d.page), false, 'the field folded');
});

test('a playlist found by its tracks opens with the words gone and the tracks they find marked', async () => {
  await at(d.page, '#/playlists');
  await typeIn(d.page, 'moon');
  // Found by its track once the server's deep search answers: its why line says so.
  await d.page.getByText(/Moon river/).first().waitFor({ timeout: 10_000 });
  await d.page.getByLabel('View Night drives').first().click();
  await until(async () => (await words(d.page)) === '', 'the words stayed with the playlist open');
  const marked = d.page.locator('[data-track="t2"] .active');
  await marked.first().waitFor({ timeout: 10_000 });
  assert.equal(await d.page.locator('[data-track="t1"] .active').count(), 0, 'only the track the words find is marked');
  assert.equal(await d.page.locator('[data-track]').count(), 2, 'the list is not narrowed');
});
