// Two accounts on one device (decision 0101, points 13–15): the device's settings stay the device's; the person's own
// (pinned items, libraries switched off…) are each account's, hidden while another is signed in and back at its next
// sign-in; signed out, the page is nobody's.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, inModule, signIn, signOut, storage } from './browser.mjs';

const PREFS = '/src/lib/prefs.svelte.ts';
const set = (page, values) =>
  inModule(
    page,
    PREFS,
    async (m, v) => {
      Object.assign(m.prefs, v);
      await new Promise((r) => setTimeout(r, 300));
    },
    values,
  );
const get = (page, keys) => inModule(page, PREFS, (m, k) => Object.fromEntries(k.map((x) => [x, JSON.parse(JSON.stringify(m.prefs[x]))])), keys);
const json = (s, k) => JSON.parse(s[k] ?? '{}');

let d;
before(async () => (d = await device()));
after(async () => {
  assert.deepEqual(d.errors, [], 'the page threw nothing');
  await d.browser.close();
});

test('the device’s settings and the person’s own are kept apart', async () => {
  await signIn(d.page, 'ann');
  await set(d.page, { theme: 'dark', pinned: ['pl-ann'], hiddenLibraries: ['cratarr/x'] });
  const s = await storage(d.page);
  assert.equal(json(s, 'cratarr-prefs').theme, 'dark');
  assert.ok(!('pinned' in json(s, 'cratarr-prefs')) && !('hiddenLibraries' in json(s, 'cratarr-prefs')), 'none of hers among the device’s');
  assert.deepEqual(json(s, 'cratarr-prefs@ann').pinned, ['pl-ann']);
  assert.deepEqual(json(s, 'cratarr-prefs@ann').hiddenLibraries, ['cratarr/x']);
  assert.equal(await d.page.evaluate(() => document.documentElement.dataset.theme), 'dark');
});

test('signed out, the page is nobody’s, and hers are kept', async () => {
  await signOut(d.page);
  const s = await storage(d.page);
  assert.equal(s['cratarr-who'], 'null');
  assert.deepEqual(json(s, 'cratarr-prefs@ann').pinned, ['pl-ann']);
});

test('another account on the same device: the device’s settings, none of hers', async () => {
  await signIn(d.page, 'bob');
  const p = await get(d.page, ['theme', 'pinned', 'hiddenLibraries']);
  assert.equal(p.theme, 'dark');
  assert.deepEqual(p.pinned, []);
  assert.deepEqual(p.hiddenLibraries, []);
  await set(d.page, { pinned: ['pl-bob'] });
  const s = await storage(d.page);
  assert.deepEqual(json(s, 'cratarr-prefs@bob').pinned, ['pl-bob']);
  assert.deepEqual(json(s, 'cratarr-prefs@ann').pinned, ['pl-ann'], 'hers untouched');
});

test('hers again at her next sign-in', async () => {
  await signOut(d.page);
  await signIn(d.page, 'ann');
  const p = await get(d.page, ['pinned', 'hiddenLibraries']);
  assert.deepEqual(p.pinned, ['pl-ann']);
  assert.deepEqual(p.hiddenLibraries, ['cratarr/x']);
});

test('the stores this run writes, kept per account, are each named for one', async () => {
  const keys = Object.keys(await storage(d.page));
  for (const plain of ['cratarr-queues', 'cratarr.scrobbleQueue', 'cratarr-prefs-account']) assert.ok(!keys.includes(plain), plain);
});
