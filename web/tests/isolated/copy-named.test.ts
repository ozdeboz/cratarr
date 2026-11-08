// Every action on a copy says which one (server `named_copy`): a record of two copies with none named answers 409, and a
// library's id holds `/`, so `from` must arrive whole.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const { deleteAlbum, deleteOwnAlbum, moveAlbum, movePreview, replaceIncoming, replacePreview } = await import('../../src/lib/api.ts');

const OWN = 'cratarr//own/cy';
type Asked = { path: string; body: unknown };
async function asked(run: () => Promise<unknown>): Promise<Asked> {
  let seen: Asked | null = null;
  globalThis.fetch = (async (path: string, init?: RequestInit) => {
    seen = { path, body: init?.body ? JSON.parse(init.body as string) : null };
    return new Response('{}', { headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch;
  await run();
  return seen!;
}
const query = (path: string) => Object.fromEntries(new URL(path, 'http://x').searchParams);

test("a move's preview names the copy and the target in its query, a library's slashes kept", async () => {
  const { path } = await asked(() => movePreview('cratarr', 'al-1', OWN, 'music', '1999'));
  assert.match(path, /^\/api\/albums\/cratarr\/al-1\/move\?/);
  assert.ok(path.includes(encodeURIComponent(OWN)), 'encoded, or the server reads another path');
  assert.deepEqual(query(path), { from: OWN, to: 'music', year: '1999' });
});

test('a move names the copy in its body', async () => {
  assert.deepEqual((await asked(() => moveAlbum('cratarr', 'al-1', 'in', 'music'))).body, { from: 'in', to: 'music' });
});

test('a delete names the copy in its body', async () => {
  const { path, body } = await asked(() => deleteAlbum('cratarr', 'al-1', OWN));
  assert.equal(path, '/api/albums/cratarr/al-1/delete');
  assert.deepEqual(body, { from: OWN });
});

test('an album of a list with no libraries names none, and the server takes its one copy', async () => {
  assert.deepEqual(query((await asked(() => movePreview('jelly', '9', undefined, 'music'))).path), { to: 'music' });
  assert.deepEqual((await asked(() => deleteAlbum('jelly', '9', undefined))).body, {});
});

test("Replace's preview names the copy and the main library it takes the place of", async () => {
  const { path } = await asked(() => replacePreview('cratarr', 'al-1', 'in', 'music'));
  assert.match(path, /^\/api\/incoming\/replace\/cratarr\/al-1\?/);
  assert.deepEqual(query(path), { from: 'in', to: 'music' });
});

test('Replace names both copies beside its cover and its carrying over', async () => {
  const { body } = await asked(() => replaceIncoming('cratarr', 'al-1', 'in', 'music', '2001', 'new', true));
  assert.deepEqual(body, { from: 'in', to: 'music', year: '2001', cover: 'new', carry: true });
});

test("a delete from the Users panel names the owner's copy, not the one a record also has in Music", async () => {
  const { path, body } = await asked(() => deleteOwnAlbum('cratarr', 'al-1', OWN));
  assert.equal(path, '/api/albums/cratarr/al-1/delete-own');
  assert.deepEqual(body, { from: OWN });
});

test("the Users panel's every act on someone's music names their library", () => {
  const src = readFileSync(new URL('../../src/lib/settings/UsersPart.svelte', import.meta.url), 'utf8');
  assert.match(src, /moveAlbum\(OWN, a\.id, u\.own_library!, to\.id\)/);
  assert.match(src, /deleteOwnAlbum\(OWN, a\.id, u\.own_library!\)/);
  assert.match(src, /deleteOne\(a, open!\.own_library!\)/);
});
