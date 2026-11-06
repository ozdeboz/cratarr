// The outbox sending for real (lib/later): a process of its own, as it stands in for `fetch` and for Svelte's `$state`.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = Object.assign(<T>(v: T) => v, { snapshot: <T>(v: T) => v });
const { later, sendLater } = await import('../../src/lib/later.svelte.ts');
const { accountLearnt } = await import('../../src/lib/account.ts');

const change = (key: string) => ({ key, method: 'POST', path: `/api/x/${key}`, body: null, type: null, at: Date.now(), account: 'ann' });

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
