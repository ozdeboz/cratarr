// A page opened again as the account it kept (lib/account, lib/scrobbles, lib/accountPrefs, the real ones): no name changes, so
// nothing that hears of a change of account hears of it. What must run at every open, once Cratarr has said who it is, runs
// all the same: a browser's scrobbling goes by the way it reads then, and the preferences are compared then.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as wait } from 'node:timers/promises';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
Object.assign(g.location as object, { assign() {} });
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });

// Opened as ann: the name kept, a play left to send, and her sign-in here.
localStorage.setItem('cratarr-who', '"ann"');
localStorage.setItem('cratarr.scrobblers@ann', JSON.stringify({ listenbrainz: { token: 'hers', name: 'ann-lb' } }));
localStorage.setItem('cratarr.scrobbleQueue@ann', JSON.stringify({ plays: [{ id: 'left', at: 1, artist: 'a', title: 't', album: null }], sent: { listenbrainz: [], lastfm: [] } }));

const asked: string[] = [];
const held: Record<string, { value: unknown; changed: number }> = { pinned: { value: ['from the phone'], changed: 5 } };
const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json' } });
globalThis.fetch = (async (input: string) => {
  const path = String(input);
  asked.push(path);
  if (path === '/api/scrobble') return json({ way: 'device' });
  if (path.endsWith('/api/me/preferences')) return json({ values: held });
  if (path.includes('listenbrainz')) return json({ status: 'ok' });
  return json({});
}) as typeof fetch;

const { accountKey, accountLearnt, pageGoing, whenAccount } = await import('../../src/lib/account.ts');
const { toSignIn } = await import('../../src/lib/reach.svelte.ts');
const { scrobbling } = await import('../../src/lib/scrobbles.svelte.ts');
const { prefs } = await import('../../src/lib/prefs.svelte.ts');
await import('../../src/lib/accountPrefs.svelte.ts');
await wait(5);

test('a page opens as the name kept, and Cratarr saying the same is no change of account', async () => {
  assert.equal(accountKey(), 'ann');
  let told = 0;
  const f = () => told++;
  whenAccount.add(f);
  accountLearnt('ann');
  whenAccount.delete(f);
  assert.equal(told, 0);
  await wait(40);
});

test("a browser's scrobbling goes by the way Cratarr says, read at an ordinary open", () => {
  assert.equal(scrobbling.way, 'device');
});

test('what waited from before goes then, and what shows of the services is read', () => {
  assert.equal(scrobbling.here.listenbrainz?.name, 'ann-lb');
  assert.ok(asked.some((a) => a.includes('listenbrainz')), 'sent');
  assert.deepEqual(JSON.parse(localStorage.getItem('cratarr.scrobbleQueue@ann')!).plays, []);
});

test('the preferences are compared at an ordinary open, with no live channel to set it off', () => {
  assert.deepEqual(prefs.pinned, ['from the phone']);
});

test('sent to sign in, the page is going: nothing for the browser to ask', () => {
  assert.equal(pageGoing(), false);
  toSignIn();
  assert.equal(pageGoing(), true);
});
