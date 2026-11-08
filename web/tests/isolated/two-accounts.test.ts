// Two accounts on one device (decision 0101, point 14; lib/prefs, lib/accountPrefs and lib/localCovers, the real ones):
// each has its own preferences here and its own record of when they changed, so another one signing in, or nobody, is
// no change of anyone's. Nothing is sent for it, nothing of one shows for another, and what an account changed and could
// not send yet is there at its return.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as wait } from 'node:timers/promises';
import { effects } from '../harness/pageGlobals.ts';

type Held = { value: unknown; changed: number };
/** Each account's preferences as the server keeps them, the session the requests carry, and each one sent. */
const server: Record<string, Record<string, Held>> = {};
let session = '';
let down = false;
const sent: { account: string; key: string; value: unknown }[] = [];
const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json' } });
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  if (down) throw new TypeError('no network');
  const path = String(input);
  const held = (server[session] ??= {});
  const key = path.match(/\/api\/me\/preferences\/(\w+)$/)?.[1];
  if (key && init?.method === 'PUT') {
    const { value, changed } = JSON.parse(String(init.body));
    sent.push({ account: session, key, value });
    if (!held[key] || changed > held[key].changed) held[key] = { value, changed };
    return json(held[key]);
  }
  return json(path.endsWith('/api/me/preferences') ? { values: held } : {});
}) as typeof fetch;

const { accountForgotten, accountLearnt } = await import('../../src/lib/account.ts');
const { HOME_HEADER, homePicture, localCovers, takeHomePicture } = await import('../../src/lib/localCovers.svelte.ts');
const { prefs } = await import('../../src/lib/prefs.svelte.ts');
const { keepDraft, keptDraft } = await import('../../src/lib/crates.svelte.ts');
const mine = effects.length;
await import('../../src/lib/accountPrefs.svelte.ts');

/** What the page's reactions would do now: this device's changes noted, and sent once their wait is over. */
async function settled() {
  for (const run of effects.slice(mine)) run();
  await wait(650);
}
async function signIn(who: string) {
  session = who;
  accountLearnt(who);
  await settled();
}
async function signOut() {
  accountForgotten();
  await settled();
}
const sentFor = (account: string) => sent.filter((s) => s.account === account);

test('the first account on a fresh device has nothing to send: a default is no value of its own', async () => {
  await signIn('ann');
  assert.deepEqual(sent, []);
});

test('a change of an account signed in reaches it', async () => {
  prefs.pinned = ['first'];
  await settled();
  assert.deepEqual(sent, [{ account: 'ann', key: 'pinned', value: ['first'] }]);
  sent.length = 0;
  await signOut();
});

test("signing out is no change: what another device changed since, this device's copy older, is taken and not overwritten", async () => {
  await signIn('ann');
  assert.deepEqual(prefs.pinned, ['first'], 'her own again');
  await signOut();
  assert.deepEqual(prefs.pinned, [], 'nobody has none of hers');
  server.ann.pinned = { value: ['from the phone'], changed: server.ann.pinned.changed + 1 };
  await signIn('ann');
  assert.deepEqual(prefs.pinned, ['from the phone']);
  assert.deepEqual(sent, [], "nothing of this device's went to her account");
  await signOut();
});

test('what an account changed out of reach waits through another one on the device, and goes at its return', async () => {
  await signIn('ann');
  down = true;
  prefs.pinned = ['made offline'];
  await settled();
  await signOut();
  down = false;
  await signIn('bo');
  assert.deepEqual(prefs.pinned, [], 'none of hers for him');
  await signOut();
  await signIn('ann');
  assert.deepEqual(prefs.pinned, ['made offline'], 'hers, as she left it');
  assert.deepEqual(sentFor('ann').at(-1), { account: 'ann', key: 'pinned', value: ['made offline'] });
  assert.deepEqual(sentFor('bo'), [], 'and nothing went for him');
  sent.length = 0;
  await signOut();
});

test('a change still waiting its moment as its account goes is that one\'s, never sent as the next one\'s', async () => {
  await signIn('ann');
  prefs.pinned = ['just now'];
  for (const run of effects.slice(mine)) run();
  await signIn('bo');
  assert.deepEqual(sentFor('bo'), []);
  assert.deepEqual(sentFor('ann'), [], 'gone from the waiting, not gone: it is still in her copy');
  await signOut();
  await signIn('ann');
  assert.deepEqual(sentFor('ann'), [{ account: 'ann', key: 'pinned', value: ['just now'] }]);
  sent.length = 0;
  await signOut();
});

test('every preference on the account is its own here, out of reach too', async () => {
  const hers = {
    hiddenLibraries: ['cratarr/hers'],
    filters: { albums: { conditions: [], on: ['flac'] } },
    radioLast: { where: 'like', mode: 'era', albums: true },
  } as const;
  await signIn('ann');
  Object.assign(prefs, hers);
  keepDraft(null, { name: 'Her crate', conditions: [] });
  await settled();
  await signOut();
  assert.equal(keptDraft(null), null, 'nobody has no draft');
  assert.deepEqual(prefs.hiddenLibraries, []);
  await signIn('bo');
  assert.equal(keptDraft(null), null, 'nor has he');
  assert.deepEqual(prefs.hiddenLibraries, []);
  assert.deepEqual(prefs.filters, {});
  assert.notEqual(prefs.radioLast.mode, 'era');
  keepDraft(null, { name: 'His crate', conditions: [] });
  await settled();
  await signOut();
  down = true;
  await signIn('ann');
  assert.deepEqual(keptDraft(null), { name: 'Her crate', conditions: [] });
  assert.deepEqual(prefs.hiddenLibraries, hers.hiddenLibraries);
  assert.deepEqual(prefs.filters.albums?.on, ['flac']);
  assert.equal(prefs.radioLast.mode, 'era');
  await signIn('bo');
  assert.deepEqual(keptDraft(null), { name: 'His crate', conditions: [] });
  down = false;
  sent.length = 0;
  await signOut();
});

test("Home's picture is the account's here too, and nobody has it", async () => {
  await signIn('ann');
  // No IndexedDB here: what is kept beside the picture is the part that runs.
  await takeHomePicture('data:image/jpeg;base64,SGVycw==').catch(() => {});
  await settled();
  const name = [...Array(localStorage.length).keys()].map((i) => localStorage.key(i)!).filter((k) => localStorage.getItem(k) === homePicture.held);
  assert.deepEqual(name, ['cratarr-home-picture@ann']);
  await signOut();
  assert.equal(homePicture.held, null);
  assert.equal(localCovers[HOME_HEADER], undefined, 'and it shows nowhere');
  await signIn('bo');
  assert.equal(homePicture.held, null, "his own, none, whatever her copy on this device");
  await signOut();
  down = true;
  await signIn('ann');
  assert.equal(homePicture.held, 'data:image/jpeg;base64,SGVycw==');
  down = false;
  await signOut();
});
