// The library on a device in the app, in a real browser, with a stand-in for the app's end of the bridge: what only runs here is the
// order of what the page sends, as its settings go by an effect, which the tests in Node have no way to run.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { device, inModule, PAGE, signIn } from './browser.mjs';

const SYNC = '/src/lib/deviceSync.svelte.ts';

let d;
before(async () => {
  d = await device();
  await signIn(d.page, 'ann');
  await d.page.addInitScript(() => {
    window.sentToApp = [];
    window.CratarrApp = { postMessage: (m) => window.sentToApp.push(JSON.parse(m)), onmessage: null };
  });
  await d.page.goto(PAGE);
  await d.page.waitForFunction(() => window.sentToApp.some((m) => m.type === 'hello'), null, { timeout: 20_000 });
});
after(async () => {
  assert.deepEqual(d.errors, [], 'the page threw nothing');
  await d.browser.close();
});

/** What the app heard of the person's answer, in order, and of the word to look. */
const heard = () =>
  d.page.evaluate(() => window.sentToApp.flatMap((m) => (m.type === 'page-prefs' ? [JSON.parse(m.prefs).indexMobile] : m.type.startsWith('index-') ? [m.type] : [])));

test('a yes reaches the app before the word to look, or the app would go by what it knew and ask again', async () => {
  await inModule(d.page, SYNC, (m) => m.answerMobile(true));
  const said = await heard();
  const look = said.indexOf('index-look');
  assert.ok(look > 0, JSON.stringify(said));
  assert.equal(said[look - 1], 'yes');
});

test('a tap reaches the app after the answer it takes back', async () => {
  await inModule(d.page, SYNC, async (m) => {
    await m.answerMobile(false);
    await m.tryIndexAgain();
  });
  const said = await heard();
  const again = said.indexOf('index-again');
  assert.ok(again > 0, JSON.stringify(said));
  assert.equal(said[again - 1], 'unasked');
});
