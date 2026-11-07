// The outbox sending for real (lib/later): a process of its own, as it stands in for `fetch` and for Svelte's `$state`.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = Object.assign(<T>(v: T) => v, { snapshot: <T>(v: T) => v });
const { later, sendLater } = await import('../../src/lib/later.svelte.ts');
const { accountLearnt } = await import('../../src/lib/account.ts');

const change = (key: string) => ({ key, method: 'POST', path: `/api/x/${key}`, body: null, type: null, at: Date.now(), account: 'ann' });
const DAY = 86_400_000;

test("a refusal in Cratarr's words lets the change go and what waited behind it; a proxy's holds them all", async () => {
  const asked: string[] = [];
  let forbidden = () => new Response('{"error":"Only its owner changes it: copy it into your own."}', { status: 403 });
  globalThis.fetch = (async (path: string) => {
    asked.push(path);
    return path.endsWith('/shared') ? forbidden() : new Response('{}');
  }) as typeof fetch;
  accountLearnt('ann');
  await new Promise((ok) => setTimeout(ok, 0));

  later.list = [change('shared'), change('next')];
  await sendLater();
  assert.deepEqual(later.list, [], 'let go, not at the head for a week');
  assert.deepEqual(asked, ['/api/x/shared', '/api/x/next']);

  asked.length = 0;
  forbidden = () => new Response('<html>Forbidden</html>', { status: 403 });
  later.list = [change('shared'), change('next')];
  await sendLater();
  assert.deepEqual(later.list.map((c) => c.key), ['shared', 'next'], 'out of reach: all wait');
  assert.deepEqual(asked, ['/api/x/shared']);
});

test('a heart of day 0, its account signed out for a week: sent at her return, as point 13 says, not let go', async () => {
  const asked: string[] = [];
  globalThis.fetch = (async (path: string) => {
    asked.push(path);
    return new Response('{}');
  }) as typeof fetch;
  later.list = [{ ...change('away'), at: Date.now() - 8 * DAY }];
  await sendLater();
  assert.deepEqual(asked, ['/api/x/away']);
  assert.deepEqual(later.list, []);
});

/** Each try of `answering`'s. */
let tries = 0;
/** What Cratarr answers from now on, and whether the change is still kept after a try. */
async function answering(status: number | null, op: ReturnType<typeof change> & { tried?: number }) {
  globalThis.fetch = (async () => {
    tries++;
    if (status === null) throw new TypeError('no network');
    return new Response('', { status });
  }) as typeof fetch;
  later.list = [op];
  await sendLater();
  return later.list[0];
}

test('the first answer to turn one away starts its week; the week past, the next one lets it go', async () => {
  const first = await answering(503, change('sick'));
  assert.ok(Math.abs(first.tried! - Date.now()) < 5_000);
  const again = await answering(503, { ...change('sick'), tried: first.tried! - 3 * DAY });
  assert.equal(again.tried, first.tried! - 3 * DAY, 'its first, not its last');
  assert.equal(await answering(503, { ...change('sick'), tried: Date.now() - 8 * DAY }), undefined);
});

test('and a week past is no reason not to try it once more: Cratarr may have mended', async () => {
  tries = 0;
  assert.equal(await answering(200, { ...change('mended'), tried: Date.now() - 8 * DAY }), undefined);
  assert.equal(tries, 1, 'gone because it was taken');
});

test('a device away, or a session over, starts no week: nothing was said of the change', async () => {
  assert.equal((await answering(null, change('away'))).tried, undefined);
  assert.equal((await answering(401, change('over'))).tried, undefined);
  const old = { ...change('old'), at: Date.now() - 30 * DAY };
  assert.ok(await answering(null, old), 'and however old, it stays');
});
