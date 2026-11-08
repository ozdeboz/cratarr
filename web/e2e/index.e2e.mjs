// The library on a device in a real browser: index.wasm as Vite serves it, IndexedDB and the door, on the rules crate's own bytes
// (e2e/standin.mjs). Each account's apart, the first seconds left alone, and with Cratarr gone a page reads what is kept, and a crate
// is what the device answers.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, inModule, PAGE, signIn } from './browser.mjs';

const SYNC = '/src/lib/deviceSync.svelte.ts';
const API = '/src/lib/api.ts';
const stands = (page) => inModule(page, SYNC, (m) => ({ at: m.indexSync.at, rev: m.indexSync.rev, shows: m.indexSync.shows }));
const standin = (page, what) => page.evaluate((w) => fetch(`/api/standin/${w}`).then((r) => r.json()), what);
const records = (page) => inModule(page, API, async (m) => (await m.getAlbums()).sources.flatMap((s) => s.albums.map((a) => a.title)));

/** `ask` again until `ok` says so of its answer. */
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
  assert.deepEqual(d.errors, [], 'the page threw nothing');
  await d.browser.close();
});

/** How many small covers this browser holds of `who`'s. */
const coversHeld = (page, who) =>
  page.evaluate(async (who) => {
    const db = await new Promise((done, failed) => {
      const r = indexedDB.open(`cratarr-index@${who}`);
      r.onsuccess = () => done(r.result);
      r.onerror = () => failed(r.error);
    });
    const n = await new Promise((done) => {
      const r = db.transaction('covers').objectStore('covers').count();
      r.onsuccess = () => done(r.result);
    });
    db.close();
    return n;
  }, who);

test('a start leaves its first seconds alone, then the index, the sounds and the covers are in this browser, under the account', async () => {
  await signIn(d.page, 'ann');
  const opened = Date.now();
  assert.equal((await stands(d.page)).rev, null);
  await until(() => stands(d.page), (s) => s.rev === 5, 'the whole');
  assert.ok(Date.now() - opened > 5_000, 'not in the first seconds');
  const kept = await d.page.evaluate(async () => {
    const db = await new Promise((done, failed) => {
      const r = indexedDB.open('cratarr-index@ann');
      r.onsuccess = () => done(r.result);
      r.onerror = () => failed(r.error);
    });
    const read = (name) =>
      new Promise((done) => {
        const r = db.transaction('parts').objectStore('parts').get(name);
        r.onsuccess = () => done(r.result);
      });
    const [index, sounds, covers] = [await read('index'), await read('sounds'), await read('covers')];
    const [indexBytes, soundsBytes] = [await read('index:bytes'), await read('sounds:bytes')];
    db.close();
    return { index: indexBytes.length, indexRev: index.rev, sounds: soundsBytes.length, soundsRev: sounds.rev, coversRev: covers.rev };
  });
  assert.deepEqual(kept, { index: 921, indexRev: 5, sounds: 45, soundsRev: 5, coversRev: 5 });
  assert.equal(await coversHeld(d.page, 'ann'), 2);
});

test('This device shows the date', async () => {
  await d.page.goto(`${PAGE}/#/settings?view=settings:device`);
  const row = d.page.getByRole('button', { name: /Library on this device/ }).or(d.page.getByText('Library on this device').locator('xpath=ancestor::*[self::li or self::div][1]'));
  await row.first().waitFor({ timeout: 20_000 });
  const year = String(new Date().getFullYear());
  assert.ok((await d.page.locator('body').innerText()).includes(year), 'a date in the row');
});

test('Cratarr gone: once it was looked for twice the Library lists what the device keeps, nothing asked of it', async () => {
  await d.page.goto(`${PAGE}/#/library`);
  await standin(d.page, 'down');
  await d.page.getByText('First Light', { exact: false }).first().waitFor({ timeout: 45_000 });
  await until(() => records(d.page), (r) => Array.isArray(r) && r.includes('Second Wind') && r.includes('Own Thing'), 'the list');
  const view = await inModule(d.page, API, (m) => m.albumView('cratarr', 'al-10'));
  assert.deepEqual(view.rows.map((r) => r.name), ['Dawn', 'Noon']);
});

test('Cratarr gone: the Crates page, a crate and a Sounds like are what this device answers, by the rules crate', async () => {
  await d.page.goto(`${PAGE}/#/crates`);
  await d.page.getByText('Hearted', { exact: true }).first().waitFor({ timeout: 45_000 });
  const said = await inModule(d.page, API, async (m) => {
    const list = await m.getCrates();
    const rows = await m.crateRows('hearted', 0, 10);
    const like = await m.crateRows('like:tr-100@10', 0, 20, undefined, undefined, undefined, { order: 'own' });
    return { crates: list.map((c) => [c.id, c.counts]), rows: rows.rows.map((r) => r.id), like: like.rows.map((r) => r.id) };
  });
  assert.deepEqual(said, { crates: [['hearted', { all: 5, only: 2, none: 3 }]], rows: ['tr-102', 'tr-101'], like: ['tr-100', 'tr-101', 'tr-104', 'tr-102', 'tr-103'] });
  const picked = await inModule(d.page, API, (m) => m.radioNext({ mode: 'similar', seed: ['tr-101'], exclude: ['tr-101'], last: 'tr-101', count: 5 }));
  assert.deepEqual(picked.tracks, [], 'nothing downloaded here: nothing, and no word');
});

test('Cratarr back: reads are its own again, and what it changed meanwhile comes by the feed, to the rev', async () => {
  await standin(d.page, 'index-moves');
  await standin(d.page, 'up');
  await until(() => coversHeld(d.page, 'ann'), (n) => n === 3, 'the cover made meanwhile');
  await until(() => stands(d.page), (s) => s.rev === 9 && s.shows === 'idle', 'the feed', 60_000);
});

test('a page opened with Cratarr gone reads what the feed left', async () => {
  await standin(d.page, 'down');
  await d.page.reload();
  await until(() => records(d.page), (r) => Array.isArray(r) && r.includes('New Day'), 'the list as of the feed');
  await standin(d.page, 'up');
});
