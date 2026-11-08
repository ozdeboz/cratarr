// Whose a change is when nobody was signed in (lib/later, the real one; decision 0101): the page that made it can name it, at
// its own first answer. Kept and left for another page it is no one's: made perhaps by someone signed out a moment before,
// it is not the next one's to send, as a favourite of theirs.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as wait } from 'node:timers/promises';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });

const asked: string[] = [];
globalThis.fetch = (async (input: string) => {
  asked.push(String(input));
  return new Response('{}');
}) as typeof fetch;

const OUTBOX = 'cratarr-kept-outbox';
const change = (key: string, account?: string) => ({ key, method: 'POST', path: `/api/x/${key}`, body: null, type: null, at: Date.now(), ...(account === undefined ? {} : { account }) });
// As the page before this one left them: a heart made as nobody, in the moments a sign-out takes, and one of hers.
localStorage.setItem(OUTBOX, JSON.stringify([change('nobodys'), change('hers', 'ann')]));

const { accountLearnt, accountKey } = await import('../../src/lib/account.ts');
const { keepAndTry, later } = await import('../../src/lib/later.svelte.ts');

const kept = () => (JSON.parse(localStorage.getItem(OUTBOX)!) as { key: string }[]).map((c) => c.key);
const notReached = () => true;

test('what a page before this one made with nobody signed in is no one’s: dropped as it is read, for whoever signs in', async () => {
  assert.equal(accountKey(), null);
  await wait(20);
  assert.deepEqual(later.list.map((c) => c.key), ['hers'], 'hers waits for her');
  assert.deepEqual(kept(), ['hers'], 'and it is gone from the store too');
});

test('what this page makes before it knows is its own: its first answer says whose', async () => {
  const made = keepAndTry(change('heart'), () => Promise.reject(new Error('not reached')), notReached);
  assert.equal(await made, null);
  assert.equal(later.list.find((c) => c.key === 'heart')?.account, undefined);
  accountLearnt('bo');
  await wait(20);
  assert.ok(asked.includes('/api/x/heart'), 'sent as his, the page having asked it under his session');
  assert.ok(!asked.includes('/api/x/nobodys'));
  assert.ok(!asked.includes('/api/x/hers'), 'and hers still is not');
  assert.deepEqual(later.list.map((c) => c.key), ['hers']);
});
