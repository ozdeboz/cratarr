// Crate drafts and Home's picture on the account (decision 0101 point 14; lib/accountPrefs, the real one): begun on one
// device they reach the account, and from there another device, as the other preferences do.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as wait } from 'node:timers/promises';
import { effects } from '../harness/pageGlobals.ts';

type Held = { value: unknown; changed: number };
/** The account's preferences, as the server keeps them, and each one sent to it. */
let held: Record<string, Held> = {};
const sent: { key: string; value: unknown }[] = [];
const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json' } });
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const path = String(input);
  const key = path.match(/\/api\/me\/preferences\/(\w+)$/)?.[1];
  if (key && init?.method === 'PUT') {
    const { value, changed } = JSON.parse(String(init.body));
    sent.push({ key, value });
    return json((held[key] = { value, changed }));
  }
  return json(path.endsWith('/api/me/preferences') ? { values: held } : {});
}) as typeof fetch;

/** A draft of a crate that is there, kept beside one for a new crate. */
const OLD = { name: 'From before', conditions: [] };

const { accountLearnt } = await import('../../src/lib/account.ts');
const { fittedCopy, HOME_HEADER, homePicture, localCovers } = await import('../../src/lib/localCovers.svelte.ts');
const { prefs, resetPrefs } = await import('../../src/lib/prefs.svelte.ts');
const { forgetDraft, keepDraft, keptDraft, startCrate } = await import('../../src/lib/crates.svelte.ts');
const mine = effects.length;
await import('../../src/lib/accountPrefs.svelte.ts');

/** What the page's reactions would do now: this device's changes noted, and sent once their wait is over. */
async function changesGo() {
  for (const run of effects.slice(mine)) run();
  await wait(650);
}
const answered = () => wait(50);

test('a crate draft begun here reaches the account, the whole of them as one value', async () => {
  accountLearnt('ann');
  await answered();
  keepDraft('cr-1', OLD);
  await changesGo();
  sent.length = 0;
  const draft = { name: 'Late nights', conditions: [] };
  keepDraft(null, draft);
  assert.deepEqual(keptDraft(null), draft, 'kept here as it goes');
  await changesGo();
  assert.deepEqual(sent, [{ key: 'crate_drafts', value: { 'cr-1': OLD, new: draft } }]);
});

test('a draft Saved or Cancelled leaves the account too', async () => {
  sent.length = 0;
  forgetDraft(null);
  assert.equal(keptDraft(null), null);
  await changesGo();
  assert.deepEqual(sent, [{ key: 'crate_drafts', value: { 'cr-1': OLD } }]);
});

test('a crate started from a genre or a grab is a draft like any other', async () => {
  sent.length = 0;
  startCrate('Grabbed', [], { words: 'dub' });
  assert.deepEqual(keptDraft(null), { name: 'Grabbed', conditions: [], words: 'dub' });
  forgetDraft(null);
});

test('resetting the look leaves crates being made alone', () => {
  const was = prefs.crateDrafts;
  resetPrefs();
  assert.deepEqual(prefs.crateDrafts, was);
});

test("Home's picture chosen here reaches the account", async () => {
  sent.length = 0;
  homePicture.held = 'data:image/jpeg;base64,AAAA';
  await changesGo();
  assert.deepEqual(sent, [{ key: 'home_picture', value: 'data:image/jpeg;base64,AAAA' }]);
});

test("another account's drafts and picture give way to the ones of the account signed in", async () => {
  held = {
    crate_drafts: { value: { new: { name: 'From the tablet', conditions: [] } }, changed: Date.now() },
    home_picture: { value: 'data:image/jpeg;base64,QUJD', changed: Date.now() },
  };
  accountLearnt('bo');
  await answered();
  assert.deepEqual(keptDraft(null), { name: 'From the tablet', conditions: [] });
  assert.equal(keptDraft('cr-1'), null, "the last account's is not shown");
  assert.equal(homePicture.held, 'data:image/jpeg;base64,QUJD');
  assert.ok(localCovers[HOME_HEADER], 'and it shows here as one chosen here does');
});

test('an account holding no picture takes the last one away here', async () => {
  held = {};
  accountLearnt('cy');
  await answered();
  assert.equal(homePicture.held, null);
  assert.equal(localCovers[HOME_HEADER], undefined);
});

test('what is held of a picture is the largest copy within what the account keeps', async () => {
  const asked: number[] = [];
  const copy = (side: number) => (asked.push(side), Promise.resolve('x'.repeat(side)));
  assert.equal((await fittedCopy([900, 600, 300], 700, copy))?.length, 600);
  assert.deepEqual(asked, [900, 600], 'smaller only as far as it takes');
  assert.equal(await fittedCopy([900, 600], 100, copy), null, 'none fits: it stays on this device');
});
