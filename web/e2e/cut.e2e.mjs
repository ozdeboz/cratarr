// The network cut, in a real browser (e2e/standin.mjs; decision 0101 point 28): what a device answers for itself, a search, the
// Crates page, a crate and Radio among what plays here; signed out, an account's things hidden and kept; a second account on the same
// device with its own and none of hers; and hers again at her next sign-in, from what was kept. The browser itself goes offline, its
// service worker with it, rather than the stand-in answering as a stopped Cratarr's proxy would.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, inModule, signIn, signOut } from './browser.mjs';

const API = '/src/lib/api.ts';

/** `ask` again until `ok` says so of its answer. */
async function until(ask, ok, what, ms = 45_000) {
  const end = Date.now() + ms;
  for (;;) {
    const now = await ask().catch((e) => e);
    if (ok(now)) return now;
    if (Date.now() > end) assert.fail(`${what}: ${now instanceof Error ? now.message : JSON.stringify(now)}`);
    await new Promise((go) => setTimeout(go, 250));
  }
}

let d;
before(async () => (d = await device()));
after(async () => {
  await d.page.context().setOffline(false);
  await d.browser.close();
  assert.deepEqual(d.errors, [], 'the page threw nothing');
});

const cut = (page, off) => page.context().setOffline(off);

/** Where each part of `who`'s index stands in this browser: none where it is not kept. Never opens a store that is not there. */
const kept = (page, who) =>
  page.evaluate(async (who) => {
    const name = `cratarr-index@${who}`;
    if (!(await indexedDB.databases()).some((db) => db.name === name)) return {};
    const db = await new Promise((done, failed) => {
      const r = indexedDB.open(name);
      r.onsuccess = () => done(r.result);
      r.onerror = () => failed(r.error);
    });
    const rev = (part) =>
      new Promise((done) => {
        const r = db.transaction('parts').objectStore('parts').get(part);
        r.onsuccess = () => done(r.result?.rev ?? null);
      });
    const out = db.objectStoreNames.contains('parts') ? { index: await rev('index'), own: await rev('own') } : {};
    db.close();
    return out;
  }, who);
const bothKept = (k) => k.index != null && k.own != null;

/** A track kept in this browser for `who`, as lib/offline lists it, its stream where the service worker finds it. */
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

/** What `who` keeps in this browser: the tracks listed and the streams held. */
const downloads = (page, who) =>
  page.evaluate(async (who) => {
    const listed = Object.keys(JSON.parse(localStorage.getItem(`cratarr-offline-index-2@${who}`) ?? '{}')).sort();
    const held = (await (await caches.open('cratarr-offline-2')).keys()).map((r) => new URL(r.url).pathname).sort();
    return { listed, held };
  }, who);

/** index.wasm compiled while there is a network, as a built page's service worker keeps the copy its first sync fetched: Vite's dev
 *  server has none to give with the browser offline. */
const warm = (page) => inModule(page, '/src/lib/deviceIndex.svelte.ts', (m) => m.newEngine().then(() => true));

/** What the page is answered with the network cut: a search, the Crates page, a crate, and Radio after a track. */
const answers = (page) =>
  inModule(page, API, async (m) => {
    const found = await m.search('noon');
    const crates = await m.getCrates();
    const hearted = await m.crateRows('hearted', 0, 10);
    const picked = await m.radioNext({ mode: 'similar', seed: ['tr-100'], exclude: ['tr-100'], last: 'tr-100', count: 5 });
    return {
      found: Object.fromEntries(Object.entries(found.groups).map(([kind, g]) => [kind, g.items.map((i) => i.track ?? i.id)])),
      crates: crates.map((c) => [c.id, c.counts]),
      hearted: hearted.rows.map((r) => r.id),
      picked: picked.tracks.map((t) => t.id).sort(),
    };
  });
const answered = (a) => !(a instanceof Error) && Array.isArray(a?.crates);

/** Hers, as first answered. */
let hers;

test('the network cut: a search, the Crates page, a crate and Radio are what this device answers, Radio among what plays here', async () => {
  const page = d.page;
  await signIn(page, 'ann');
  await until(() => kept(page, 'ann'), bothKept, 'her index and her own, kept', 60_000);
  await keepStream(page, 'ann', 'tr-101', 'al-10');
  await keepStream(page, 'ann', 'tr-102', 'al-11');
  await page.reload();
  await warm(page);
  await cut(page, true);
  hers = await until(() => answers(page), answered, 'her answers with the network cut');
  assert.deepEqual(hers.found.track, ['tr-101'], 'the track by its words, from what is kept');
  assert.deepEqual(hers.hearted, ['tr-102', 'tr-101'], 'her crate, by what she hearted');
  assert.ok(hers.picked.length > 0 && hers.picked.every((id) => ['tr-101', 'tr-102'].includes(id)), `among what plays here: ${hers.picked}`);
  await cut(page, false);
});

test('signed out, all of hers is kept: her index, her own and her downloads', async () => {
  const page = d.page;
  const before = await kept(page, 'ann');
  await signOut(page);
  assert.deepEqual(await kept(page, 'ann'), before);
  assert.deepEqual(await downloads(page, 'ann'), { listed: ['cratarr/tr-101', 'cratarr/tr-102'], held: ['/api/stream/cratarr/tr-101', '/api/stream/cratarr/tr-102'] });
});

test('a second account on the same device: his own index and crates, none of her downloads, hers kept apart', async () => {
  const page = d.page;
  const was = await kept(page, 'ann');
  await signIn(page, 'bob');
  await until(() => kept(page, 'bob'), bothKept, 'his index and his own, kept apart from hers', 60_000);
  await warm(page);
  await cut(page, true);
  const his = await until(() => answers(page), answered, 'his answers with the network cut');
  assert.deepEqual(his.found, hers.found, 'the same library');
  assert.deepEqual(his.hearted, ['tr-100'], 'his crate, by what he hearted');
  assert.deepEqual(his.picked, [], 'none of hers plays for him');
  assert.deepEqual(await inModule(page, '/src/lib/offline.svelte.ts', (m) => Object.keys(m.offline.kept)), [], 'her downloads are not listed for him');
  await cut(page, false);
  assert.deepEqual(await kept(page, 'ann'), was, 'hers as she left it');
  await signOut(page);
});

test('hers again at her next sign-in, from what was kept: nothing fetched whole, and with the network cut the same answers', async () => {
  const page = d.page;
  const { index } = await kept(page, 'ann');
  // What the page asks of the library: each part, and Cratarr's word on what changed since a rev.
  const asked = [];
  const note = (r) => {
    const u = new URL(r.url());
    if (u.pathname.startsWith('/api/index')) asked.push(u.pathname);
    if (u.pathname === '/api/changes' && u.searchParams.has('index')) asked.push(`changed since ${u.searchParams.get('index')}`);
  };
  page.on('request', note);
  await signIn(page, 'ann');
  // The covers are asked at every look, whole or by their changes: by then it has looked.
  await until(async () => asked, (a) => a.some((p) => p.startsWith('/api/index/covers')), "the sync's first look", 60_000);
  page.off('request', note);
  assert.ok(asked.includes(`changed since ${index}`), `asked from the rev she kept, ${index}: ${asked}`);
  assert.deepEqual(
    asked.filter((p) => p.startsWith('/api/index') && !p.endsWith('/changes')),
    [],
    'nothing fetched whole again: not the index, not its sounds, not her own',
  );
  await warm(page);
  await cut(page, true);
  assert.deepEqual(await until(() => answers(page), answered, 'hers again with the network cut'), hers);
  await cut(page, false);
});
