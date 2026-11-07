// The queue kept on the device (decision 0101, points 14 and 19), in a real browser: taken up where Cratarr cannot give it back,
// and what Play On then hears. What the player does with its sound is tests/isolated's: the stand-in has no streams.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, inModule, signIn } from './browser.mjs';

const PLAYER = '/src/lib/localPlayer.svelte.ts';
const COPY = 'cratarr//incoming';
const song = (id, from) => ({ entry: `e-${id}`, source: 'cratarr', id, name: id, artist: null, album: null, album_id: null, duration_secs: 200, ...(from ? { from } : {}) });
const kept = (ahead) => ({
  id: 'q-kept',
  name: 'On the road',
  renamed: false,
  updated: 1,
  tracks: [song('a', COPY), song('b', COPY)],
  index: 1,
  position: 42,
  playing: true,
  shuffled: false,
  repeat: 'off',
  radio: false,
  radioBy: 'track',
  radioCount: 5,
  skipFavourites: false,
  autoplay: null,
  ahead,
});

const held = (page) => inModule(page, PLAYER, (m) => ({ tracks: m.local.queue.map((t) => [t.id, t.from]), index: m.local.index, position: m.local.position, paused: m.local.paused }));
const asked = (page, path) => page.evaluate((p) => fetch(`/api/standin/${p}`).then((r) => r.json()), path);

/** What the player told Cratarr of `page`'s queue, once it has. */
async function toldOfIt(page) {
  for (let i = 0; i < 160; i++) {
    const told = (await asked(page, 'reports')).filter((r) => r.queue?.length);
    if (told.length) return told.at(-1);
    await new Promise((go) => setTimeout(go, 250));
  }
  assert.fail('Cratarr was told of no queue');
}

async function waitForQueue(page) {
  for (let i = 0; i < 80; i++) {
    if ((await held(page)).tracks.length) return;
    await new Promise((go) => setTimeout(go, 250));
  }
  assert.fail('no queue in the player');
}

let d;
before(async () => (d = await device()));
after(async () => {
  assert.deepEqual(d.errors, [], 'the page threw nothing');
  await d.browser.close();
});

test('a queue ahead of Cratarr: the page opens on it, paused where it was, and its first report is how Play On takes it up', async () => {
  await signIn(d.page, 'ann');
  await d.page.evaluate((q) => localStorage.setItem('cratarr-queue-now@ann', JSON.stringify(q)), kept(true));
  await d.page.reload();
  await waitForQueue(d.page);
  const now = await held(d.page);
  assert.deepEqual(now.tracks, [['a', COPY], ['b', COPY]]);
  assert.equal(now.index, 1);
  assert.equal(now.position, 42);
  assert.equal(now.paused, true);
  const told = await toldOfIt(d.page);
  assert.equal(told.index, 1);
  assert.equal(told.entry, 'e-b');
  assert.equal(told.position_ms, 42_000);
  assert.deepEqual(told.queue.map((t) => [t.id, t.from]), [['a', COPY], ['b', COPY]], 'with the copy each plays from');
});

test('told, the copy is in step again: not ahead of Cratarr any more', async () => {
  for (let i = 0; i < 80; i++) {
    const k = await d.page.evaluate(() => JSON.parse(localStorage.getItem('cratarr-queue-now@ann')));
    if (k?.ahead === false) return;
    await new Promise((go) => setTimeout(go, 250));
  }
  assert.fail('still ahead');
});

test('Cratarr gone as the page opens: the copy in step is taken up all the same, and Play On has it once Cratarr answers', async () => {
  await asked(d.page, 'down');
  await d.page.reload();
  await waitForQueue(d.page);
  const now = await held(d.page);
  assert.equal(now.index, 1);
  assert.equal(now.position, 42);
  assert.equal(now.paused, true);
  const before = (await asked(d.page, 'reports')).length;
  await asked(d.page, 'up');
  for (let i = 0; i < 160; i++) {
    const after = await asked(d.page, 'reports');
    if (after.length > before && after.at(-1).index === 1) return assert.equal(after.at(-1).position_ms, 42_000);
    await new Promise((go) => setTimeout(go, 250));
  }
  assert.fail('Cratarr heard nothing');
});
