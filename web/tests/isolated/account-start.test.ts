// Whose a page is from its first line (lib/account, the real one; decision 0101): every store opens under that name
// before anything has been asked, so a name kept from before must never be taken for the one who just came.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
/** What the page's listeners would hear, and how often it started over. */
const listeners: Record<string, ((e: unknown) => void)[]> = {};
let reloads = 0;
const left: string[] = [];
const worker: unknown[] = [];
g.window = globalThis;
g.addEventListener = (type: string, f: (e: unknown) => void) => void (listeners[type] ??= []).push(f);
Object.assign(g.location as object, { reload: () => void reloads++, assign: (to: string) => void left.push(to) });
Object.defineProperty(globalThis, 'navigator', { value: { serviceWorker: { controller: { postMessage: (m: unknown) => void worker.push(m) }, addEventListener() {} } }, configurable: true });

const WHO = 'cratarr-who';
let round = 0;
/** A page opening, the store's name as `kept`. */
async function opened(kept: string | null) {
  for (const k of Object.keys(listeners)) delete listeners[k];
  worker.length = 0;
  if (kept === null) localStorage.removeItem(WHO);
  else localStorage.setItem(WHO, kept);
  return (await import(`../../src/lib/account.ts?page=${++round}`)) as typeof import('../../src/lib/account.ts');
}
const fire = (type: string, e: unknown) => {
  for (const f of listeners[type] ?? []) f(e);
};

test('a page opens as the account kept, so it works out of reach', async () => {
  assert.equal((await opened('"ann"')).accountKey(), 'ann');
});

test('signed out is kept too: nobody, never the one before, whatever an older copy in the kept store says', async () => {
  localStorage.setItem('cratarr-kept-account', '"ann"');
  const a = await opened('null');
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(a.accountKey(), null);
  localStorage.removeItem('cratarr-kept-account');
});

test("Cratarr's sign-in page says nobody in the page's own words, and that no sign-out is left to tell", () => {
  const server = readFileSync(new URL('../../../server/src/serve/signin.rs', import.meta.url), 'utf8');
  const account = readFileSync(new URL('../../src/lib/account.ts', import.meta.url), 'utf8');
  const script = server.slice(server.indexOf('<script nonce='), server.indexOf('</script>'));
  assert.ok(script.includes(`localStorage.setItem('${WHO}', 'null')`));
  assert.ok(account.includes(`const LS = '${WHO}';`), 'the same name');
  const owed = account.match(/const OWED = '([^']+)';/)![1];
  assert.ok(script.includes(`localStorage.removeItem('${owed}')`));
});

test('the service worker hears whose the page is from its first line, before any read', async () => {
  await opened('"ann"');
  assert.deepEqual(worker, [{ type: 'account', name: 'ann' }]);
  await opened('null');
  assert.deepEqual(worker, [{ type: 'account', name: null }]);
});

test('another tab signing out or in starts this page over: what it holds in memory is the one before\'s', async () => {
  const a = await opened('"ann"');
  reloads = 0;
  localStorage.setItem(WHO, 'null');
  fire('storage', { key: 'cratarr-prefs' });
  assert.equal(reloads, 0, 'another store changing is nothing to it');
  assert.equal(a.pageGoing(), false);
  fire('storage', { key: WHO });
  assert.equal(reloads, 1);
  assert.equal(a.pageGoing(), true, 'the browser is not to ask, music playing or not');
  localStorage.setItem(WHO, '"bo"');
  fire('storage', { key: WHO });
  assert.equal(reloads, 2);
});

test('another tab saying the same account is nothing to it', async () => {
  await opened('"ann"');
  reloads = 0;
  fire('storage', { key: WHO });
  assert.equal(reloads, 0);
});

test('a page back from the browser\'s history after a sign-out starts over', async () => {
  await opened('"ann"');
  reloads = 0;
  fire('pageshow', { persisted: false });
  assert.equal(reloads, 0, 'a page loaded as usual');
  fire('pageshow', { persisted: true });
  assert.equal(reloads, 0, 'the same account: as it was');
  localStorage.setItem(WHO, 'null');
  fire('pageshow', { persisted: true });
  assert.equal(reloads, 1);
});

test("a page back from history after its own sign-out starts over: its stores are nobody's, its heap is hers", async () => {
  const a = await opened('"ann"');
  a.accountForgotten();
  reloads = 0;
  fire('pageshow', { persisted: true });
  assert.equal(reloads, 1);
  fire('pageshow', { persisted: false });
  assert.equal(reloads, 1, 'one opened as nobody, shown as any page is, stays');
});

test('Cratarr asking for its sign-in is nobody from then on, before the page leaves, the worker told', async () => {
  const { accountKey, accountLearnt } = await import('../../src/lib/account.ts');
  const { onSignInAsked, toSignIn } = await import('../../src/lib/reach.svelte.ts');
  accountLearnt('ann');
  worker.length = 0;
  onSignInAsked(() => true);
  toSignIn();
  assert.equal(accountKey(), null, 'in the app, playing Local, the page stays and is nobody');
  assert.equal(localStorage.getItem(WHO), 'null');
  assert.deepEqual(worker, [{ type: 'account', name: null }]);
  assert.deepEqual(left, []);
  accountLearnt('ann');
  onSignInAsked(() => false);
  toSignIn();
  assert.equal(accountKey(), null);
  assert.equal(left.length, 1, 'and in a browser it goes to the sign-in page');
});
