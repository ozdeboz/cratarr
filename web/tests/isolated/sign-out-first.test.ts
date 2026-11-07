// What goes first at a browser's sign-out (decision 0101, point 13: "they go first if the device is online"; lib/signOut, with
// the real stores): what waits of the account, its changes, its plays and its scrobbles, each answered before the session
// they are told under ends.
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
g.MediaSource = { isTypeSupported: () => true };
Object.assign(g.location as object, { assign() {}, reload() {} });
/** The page's `beforeunload`, as the browser would call it: whether it asks "Leave site?". */
let beforeUnload: ((e: Event) => void) | null = null;
g.addEventListener = (type: string, f: (e: Event) => void) => void (type === 'beforeunload' && (beforeUnload = f));
const asksToStay = () => {
  const e = new Event('beforeunload', { cancelable: true });
  beforeUnload!(e);
  return e.defaultPrevented;
};
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });

/** What was asked of whom, and when each was answered. Cratarr is out of reach until the sign-out: any answer would have the
 *  stores send what waits, and that is the sign-out's to do here. */
const steps: string[] = [];
let reached = false;
/** Whether the page was going by the time it asked Cratarr to end the session: the answer may never come. */
let goingAsItAsked = false;
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const path = String(input);
  if (!reached) throw new TypeError('no network');
  if (init?.method !== 'POST') return new Response('{}');
  steps.push(`asked ${path}`);
  if (path === '/signout') goingAsItAsked = pageGoing();
  await wait(15);
  steps.push(`answered ${path}`);
  if (path === '/api/plays') return new Response(JSON.stringify({ done: JSON.parse(String(init.body)).events.map((e: { id: string }) => e.id) }));
  return new Response('{}');
}) as typeof fetch;

const at = Date.now();
localStorage.setItem('cratarr-who', '"ann"');
localStorage.setItem('cratarr-kept-outbox', JSON.stringify([{ key: 'heart', method: 'POST', path: '/api/x/heart', body: null, type: null, at, account: 'ann' }]));
localStorage.setItem('cratarr-kept-plays', JSON.stringify([{ id: 'p1', source: 'cratarr', track: 't', at, ended: 'finished', counts: true, heard_secs: 100, length_secs: 100, player: null, context: null, account: 'ann' }]));
localStorage.setItem('cratarr.scrobblers@ann', JSON.stringify({ listenbrainz: { token: 'hers', name: 'ann-lb' } }));
localStorage.setItem('cratarr.scrobbleQueue@ann', JSON.stringify({ plays: [{ id: 's1', at: Math.floor(at / 1000), artist: 'a', title: 't', album: null, duration_secs: 100 }], sent: { listenbrainz: [], lastfm: [] } }));

const { pageGoing } = await import('../../src/lib/account.ts');
const { reach } = await import('../../src/lib/reach.svelte.ts');
reach.away = true;
await import('../../src/lib/output.svelte.ts');
const { local } = await import('../../src/lib/localPlayer.svelte.ts');
await import('../../src/lib/scrobbles.svelte.ts');
const { signOut } = await import('../../src/lib/signOut.svelte.ts');
await wait(20);

test('music playing, closing the page asks first', () => {
  Object.assign(local, { index: 0, paused: false });
  assert.equal(asksToStay(), true);
});

test('it all goes before the session it is told under ends', async () => {
  assert.deepEqual(steps, [], 'nothing has gone yet');
  reached = true;
  reach.away = false;
  await signOut(false);
  const ended = steps.indexOf('asked /signout');
  assert.ok(ended >= 0);
  for (const told of ['/api/x/heart', '/api/plays', 'https://api.listenbrainz.org/1/submit-listens']) {
    const answered = steps.indexOf(`answered ${told}`);
    assert.ok(answered >= 0, `${told} went: ${steps.join(', ')}`);
    assert.ok(answered < ended, `and was answered first: ${steps.join(', ')}`);
  }
});

test('a sign-out asks nothing, music playing or not: Cancel would leave her session, her music and nobody’s stores', () => {
  assert.equal(asksToStay(), false);
  assert.equal(goingAsItAsked, true, 'from the moment it began: Cratarr may never answer');
});
