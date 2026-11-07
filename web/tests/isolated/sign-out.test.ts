// Signing out of a browser (lib/signOut, lib/account, lib/me, the real ones; decision 0101): only Cratarr can end a session, as
// its cookie is out of the page's reach, so a sign-out that did not reach it is owed, and nothing it answers until then is
// anyone's: the next open would otherwise be the account's, live, with nobody signed in and nobody told.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as wait } from 'node:timers/promises';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
const listeners: Record<string, (() => void)[]> = {};
const left: string[] = [];
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
g.addEventListener = (type: string, f: () => void) => void (listeners[type] ??= []).push(f);
Object.assign(g.location as object, { reload() {}, assign: (to: string) => void left.push(to) });
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });

/** Whether Cratarr can be reached, what was asked of it, and the session it holds. */
let down = false;
let session: string | null = 'ann';
/** The worker answers /api/me with its last copy, Cratarr out of reach. */
let keptMe = false;
const asked: string[] = [];
const json = (v: unknown, status = 200) => new Response(JSON.stringify(v), { status, headers: { 'content-type': 'application/json' } });
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const path = String(input);
  asked.push(`${init?.method ?? 'GET'} ${path}`);
  if (down) throw new TypeError('no network');
  if (path === '/signout') {
    session = null;
    // As a browser gives a redirect not followed: no status of its own, not `ok`.
    return { ok: false, status: 0, type: 'opaqueredirect' };
  }
  if (keptMe && path === '/api/me') return new Response(JSON.stringify({ name: 'ann', admin: false, permissions: {} }), { headers: { 'X-Cratarr-Kept': '1' } });
  if (path === '/api/me') return session ? json({ name: session, admin: false, permissions: {} }) : json({ error: 'Sign in to Cratarr.' }, 401);
  return json({});
}) as typeof fetch;

const { accountKey, accountLearnt } = await import('../../src/lib/account.ts');
const { getCrates, getSettings } = await import('../../src/lib/api.ts');
const { refusedFor } = await import('../../src/lib/reach.svelte.ts');
const { loadMe } = await import('../../src/lib/me.svelte.ts');
const { signOut } = await import('../../src/lib/signOut.svelte.ts');

const OWED = 'cratarr-sign-out-owed';

test('a sign-out that reaches Cratarr goes to its sign-in page, with nothing owed', async () => {
  accountLearnt('ann');
  await signOut(false);
  assert.equal(accountKey(), null);
  assert.ok(asked.includes('POST /signout'));
  assert.equal(session, null);
  assert.equal(localStorage.getItem(OWED), null);
  assert.deepEqual(left, ['/signin']);
});

test('one that does not reach it stays owed: the page opens as nobody, not as the account whose session stands', async () => {
  session = 'ann';
  left.length = 0;
  accountLearnt('ann');
  down = true;
  await signOut(false);
  assert.equal(accountKey(), null);
  assert.equal(localStorage.getItem(OWED), '1');
  assert.equal(session, 'ann', 'the session stands: only Cratarr can end it');
  assert.deepEqual(left, ['/'], 'not to the browser’s error page: the app’s own, as nobody');
});

test('owed, nothing Cratarr answers is anyone’s, /api/me least of all', async () => {
  down = false;
  asked.length = 0;
  left.length = 0;
  accountLearnt('ann');
  assert.equal(accountKey(), null, 'an answer asked for before does not take her up');
  await assert.rejects(getCrates(), 'nothing is asked under the session, in the background');
  await assert.rejects(getSettings(), 'or for what one taps');
  assert.deepEqual(asked, []);
  assert.ok(refusedFor() > 0, 'the live channel, Play On and the shelf ask for themselves, and go by this');
  await loadMe();
  await wait(20);
  assert.equal(accountKey(), null, 'her session stands, and is not taken up');
  assert.equal(asked.indexOf('GET /api/me'), -1, 'nor asked');
});

test('Cratarr told, at last: nothing owed, and on to the sign-in page', async () => {
  assert.equal(session, null, 'loadMe told it first');
  assert.equal(localStorage.getItem(OWED), null);
  assert.deepEqual(left, ['/signin']);
});

test('still out of reach it stays owed, and the first answer ends it', async () => {
  session = 'ann';
  accountLearnt('ann');
  down = true;
  left.length = 0;
  await signOut(false);
  assert.deepEqual(left, ['/']);
  left.length = 0;
  await loadMe();
  assert.equal(localStorage.getItem(OWED), '1');
  assert.deepEqual(left, []);
  down = false;
  for (const f of listeners.online ?? []) f();
  await wait(20);
  assert.equal(session, null);
  assert.equal(localStorage.getItem(OWED), null);
  assert.deepEqual(left, ['/signin']);
});

test('a kept answer teaches nothing: it is the worker’s last copy, and says who was', async () => {
  assert.equal(accountKey(), null);
  keptMe = true;
  // Past the moment a kept answer is only a slow one (lib/reach).
  const now = Date.now;
  Date.now = () => now() + 60_000;
  await loadMe();
  Date.now = now;
  keptMe = false;
  assert.equal(accountKey(), null);
  assert.equal(localStorage.getItem('cratarr-who'), 'null');
});
