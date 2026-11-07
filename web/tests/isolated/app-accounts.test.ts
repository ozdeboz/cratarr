// The page in the app, two accounts on one device (decision 0101, points 13 to 15; lib/app, lib/coversAway, lib/shelfStore,
// lib/signOut, the real ones, on a stand-in for the app's end of the bridge): what the app holds is told with whose it
// is, and the page goes by that, never by its own account alone.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as wait } from 'node:timers/promises';
import { effects } from '../harness/pageGlobals.ts';
import { rowsOf } from '../harness/fakeIdb.ts';

const g = globalThis as Record<string, unknown>;
/** Every message the page sent the app, and each list of covers it asked the worker to keep. */
const toTheApp: Record<string, unknown>[] = [];
const coverLists: string[][] = [];
const bridge: { postMessage(m: string): void; onmessage: ((e: { data: string }) => void) | null } = { postMessage: (m) => void toTheApp.push(JSON.parse(m)), onmessage: null };
g.CratarrApp = bridge;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
const worker = { postMessage: (m: { type: string; urls?: string[] }) => void (m.type === 'keep-covers' && coverLists.push(m.urls!)) };
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { ready: Promise.resolve({ active: worker }), controller: worker, addEventListener() {} } }, configurable: true });
let goneTo: string | null = null;
Object.assign(g.location as object, { assign: (to: string) => void (goneTo = to), reload() {} });

/** What Cratarr was asked to do, in order. */
const steps: string[] = [];
const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json' } });
let me: () => Response | Promise<Response> = () => json({ name: 'ann', admin: false, permissions: {} });
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const path = String(input);
  if (init?.method === 'POST') steps.push(`POST ${path}`);
  if (path === '/api/me') return me();
  if (path.startsWith('/api/albums/')) return json({ title: 'A record', artist: null, year: null, genres: [], rows: [] });
  return json({});
}) as typeof fetch;

const { accountKey, accountLearnt, signOutBegun, signOutOwed, signOutTold } = await import('../../src/lib/account.ts');
const { inApp } = await import('../../src/lib/app.svelte.ts');
const { keepCoversAway } = await import('../../src/lib/coversAway.svelte.ts');
const { shelf } = await import('../../src/lib/shelfStore.svelte.ts');
const { signOut } = await import('../../src/lib/signOut.svelte.ts');
const { loadMe, me: signedIn } = await import('../../src/lib/me.svelte.ts');
const { prefs } = await import('../../src/lib/prefs.svelte.ts');
const { keepDraft } = await import('../../src/lib/crates.svelte.ts');
const { kept, sharesShow } = await import('../../src/lib/kept.svelte.ts');

/** The app telling the page something. */
const fromApp = (m: Record<string, unknown>) => bridge.onmessage!({ data: JSON.stringify(m) });
const track = (album: string) => ({ source: 'cratarr', id: `t-${album}`, name: 'One', artist: null, album: 'A', album_id: album, added: 1, download: { id: 'x', state: 'done', bytes: 1, percent: 100 } });
const tellDownloads = (who: string, ...albums: string[]) => fromApp({ type: 'downloads', tracks: albums.map(track), bytes: 0, who });
const turns = () => wait(80);
const react = async () => {
  for (const run of effects) run();
  await turns();
};
keepCoversAway();

test('this is the page in the app', () => assert.ok(inApp));

test("a list of downloads is the account's it says: the covers follow it only while it is the page's own", async () => {
  accountLearnt('ann');
  await turns();
  tellDownloads('@bo', 'al-his');
  await react();
  assert.deepEqual(coverLists, [], "another's list asks for no covers under this account");
  tellDownloads('@ann', 'al-hers');
  await react();
  assert.deepEqual(coverLists, [['/api/albums/cratarr/al-hers/cover']]);
});

test('two accounts holding the same records each have their covers kept', async () => {
  coverLists.length = 0;
  accountLearnt('bo');
  await turns();
  tellDownloads('@bo', 'al-hers');
  await react();
  assert.deepEqual(coverLists, [['/api/albums/cratarr/al-hers/cover']], 'the same list, for another cache');
});

test("the shelf follows the app's list only while it is the shelf's account's", async () => {
  await turns();
  assert.equal(shelf.who, '@bo');
  tellDownloads('@ann', 'al-hers-only');
  await react();
  assert.deepEqual(Object.keys(shelf.tracks).filter((k) => k.includes('al-hers-only')), []);
  assert.equal([...rowsOf('cratarr-shelf@bo', 'tracks').keys()].filter((k) => String(k).includes('al-hers-only')).length, 0);
  tellDownloads('@bo', 'al-his');
  await react();
  assert.ok(Object.keys(shelf.tracks).some((k) => k.includes('t-al-his')));
});

test("what the page keeps goes to the app apart: the device's, and the person's own, drafts among them, under whose they are", async () => {
  toTheApp.length = 0;
  prefs.pinned = ['his'];
  keepDraft(null, { name: 'His crate', conditions: [] });
  await react();
  const told = toTheApp.filter((m) => m.type === 'page-prefs').at(-1)!;
  assert.equal(told.who, '@bo');
  const own = JSON.parse(String(told.mine));
  assert.deepEqual(own.pinned, ['his']);
  assert.equal(own.crateDrafts.new.name, 'His crate');
  const device = JSON.parse(String(told.prefs));
  assert.ok(!('pinned' in device) && !('crateDrafts' in device), "nothing of his in the device's");
});

test('the app hears whose it is as soon as the page does', async () => {
  toTheApp.length = 0;
  await loadMe();
  assert.equal(accountKey(), 'ann');
  assert.deepEqual(toTheApp.filter((m) => m.type === 'account'), [{ type: 'account', name: 'ann' }]);
});

test('with sign-in off there is one user, and the app hears so', async () => {
  toTheApp.length = 0;
  const signedIn = me;
  me = () => new Response(JSON.stringify({ error: 'Signing in is off: there is one user.' }), { status: 404 });
  await loadMe();
  assert.equal(accountKey(), '');
  assert.deepEqual(toTheApp.filter((m) => m.type === 'account'), [{ type: 'account', name: '' }]);
  me = signedIn;
  await loadMe();
});

test('an answer asked for before a sign-out, landing after it began, is nobody’s: nothing of it taken up, and the app not told', async () => {
  const answered = me;
  accountLearnt('ann');
  await loadMe();
  let answer = (_: Response) => {};
  let asked = false;
  me = () => new Promise<Response>((done) => ((asked = true), (answer = done)));
  const answering = loadMe();
  await turns();
  assert.ok(asked);
  signOutBegun();
  toTheApp.length = 0;
  answer(json({ name: 'ann', admin: true, permissions: {} }));
  await answering;
  assert.equal(accountKey(), null);
  assert.deepEqual(toTheApp.filter((m) => m.type === 'account'), [], 'it would sign her back in');
  assert.equal(signedIn.admin, false, 'what it says of her is not read in');
  signOutTold();
  accountLearnt('ann');
  me = answered;
});

test("a session ended while the app plays Local or another server: the page stays, and the app is told it is nobody's", async () => {
  toTheApp.length = 0;
  fromApp({ type: 'app', version: '1', mode: 'device' });
  const signedIn = me;
  me = () => new Response(JSON.stringify({ error: 'Sign in to Cratarr.' }), { status: 401, headers: { 'WWW-Authenticate': 'Cookie realm="Cratarr", form-action="/signin"' } });
  await loadMe();
  assert.equal(accountKey(), null);
  assert.equal(goneTo, null, 'Local plays on: the sign-in page waits for Cratarr’s mode');
  assert.deepEqual(toTheApp.filter((m) => m.type === 'nobody'), [{ type: 'nobody' }]);
  me = signedIn;
  fromApp({ type: 'app', version: '1', mode: 'cratarr' });
  assert.match(String(goneTo), /^\/signin/, 'the sign-in that waited goes once Cratarr’s mode comes');
  goneTo = null;
  accountLearnt('ann');
});

test("signing out in the app: the app first, its answer awaited, then the page is nobody's, Cratarr's session ends, and the app's own page opens", async () => {
  toTheApp.length = 0;
  steps.length = 0;
  goneTo = null;
  const done = signOut(true);
  await turns();
  assert.deepEqual(toTheApp.filter((m) => m.type === 'sign-out'), [{ type: 'sign-out', remove: true }]);
  assert.equal(accountKey(), 'ann', 'nothing of hers is let go before what waits has gone');
  assert.deepEqual(steps, [], 'and the session still lives, for what waits to go with');
  fromApp({ type: 'signed-out' });
  await done;
  assert.equal(accountKey(), null);
  assert.deepEqual(steps, ['POST /signout']);
  assert.equal(goneTo, '/');
});

test('Cratarr out of reach, the app ends the sessions itself at its next load: nothing is owed for the page to tell', async () => {
  accountLearnt('ann');
  goneTo = null;
  const reachable = globalThis.fetch;
  globalThis.fetch = (async () => {
    throw new TypeError('no network');
  }) as typeof fetch;
  const done = signOut(false);
  await turns();
  fromApp({ type: 'signed-out' });
  await done;
  globalThis.fetch = reachable;
  assert.equal(signOutOwed(), false, 'owed, every read of a page signed in again later would be turned back');
  assert.equal(goneTo, '/');
});

test('each account’s share shows from the first that is not the one here: his limit is used, and the reason is hers', () => {
  const share = (account: string, here: boolean) => ({ account, bytes: 1, here });
  kept.accounts = [];
  assert.equal(sharesShow(), false);
  kept.accounts = [share('@ann', true)];
  assert.equal(sharesShow(), false, 'hers alone, and she is here');
  kept.accounts = [share('@ann', false)];
  assert.equal(sharesShow(), true);
  kept.accounts = [share('@ann', true), share('@bo', false)];
  assert.equal(sharesShow(), true);
  kept.accounts = [];
});
