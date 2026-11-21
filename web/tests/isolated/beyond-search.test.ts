// Beyond's search (SCENARIOS-LOOKUPS LK.D8, its page half; global search and Find more music): what Cratarr keeps for the
// words is read; where nothing is kept, the search is sent (`POST /api/search/beyond`), whose answer is the rows (200) or
// its job's task (202), followed by its id: the rows show as each source answers (the task's `result` while it runs) and
// whole at its end. It works with today's answer (a 200 from the read) and the server's half (404, then 202 and a task).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { searchBeyond } = await import('../../src/lib/api.ts');
const { emit } = await import('../../src/lib/events.svelte.ts');

const album = (title: string, source: 'musicbrainz' | 'discogs') => ({ title, artist: 'Warp', year: 1999, cover: null, sources: [source] });
const rows = (...albums: ReturnType<typeof album>[]) => ({ albums, artists: [] });
const task = (id: number, finished: number | null, result: unknown, error: string | null = null) => ({
  id,
  kind: 'search',
  preview: false,
  started: 1,
  finished,
  step: null,
  error,
  result,
});
const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

function server(answers: Record<string, (() => Response)[]>) {
  const asked: { key: string; body?: unknown }[] = [];
  (globalThis as { fetch: unknown }).fetch = async (path: string, init?: { method?: string; body?: string }) => {
    const key = `${init?.method ?? 'GET'} ${path.split('?')[0]}`;
    if (path === '/api/tasks') return json(200, { tasks: [] });
    asked.push({ key, body: init?.body ? JSON.parse(init.body) : undefined });
    const next = answers[key]?.shift();
    if (!next) throw new Error(`nothing stood in for ${key}`);
    return next();
  };
  return asked;
}
const settle = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};
const GET = 'GET /api/search/beyond';
const POST = 'POST /api/search/beyond';

test('an answer kept for the words is read, and nothing is sent', async () => {
  const asked = server({ [GET]: [() => json(200, rows(album('Warp Films', 'musicbrainz')))] });
  const r = await searchBeyond('warp');
  assert.deepEqual(r.albums.map((a) => a.title), ['Warp Films']);
  assert.deepEqual(asked.map((a) => a.key), [GET]);
});

test('none kept: the search is sent with its words and its page, and an answer is shown at once', async () => {
  const asked = server({ [GET]: [() => json(404, { error: 'Not searched yet.' })], [POST]: [() => json(200, rows(album('Warp Records', 'discogs')))] });
  const r = await searchBeyond('warp', 50, 50);
  assert.deepEqual(r.albums.map((a) => a.title), ['Warp Records']);
  assert.deepEqual(asked.map((a) => a.key), [GET, POST]);
  assert.deepEqual(asked[1].body, { q: 'warp', limit: 50, offset: 50 });
});

test("a search's job is followed: its rows show as each source answers, and whole at its end", async () => {
  server({ [GET]: [() => json(404, {})], [POST]: [() => json(202, { task: task(31, null, null) })] });
  const shown: string[][] = [];
  const got = searchBeyond('warp', 20, 0, (r) => shown.push(r.albums.map((a) => a.title)));
  await settle();
  emit('task', task(31, null, rows(album('Warp Films', 'musicbrainz'))));
  emit('task', task(31, 2, rows(album('Warp Films', 'musicbrainz'), album('Warp Records', 'discogs'))));
  const r = await got;
  assert.deepEqual(r.albums.map((a) => a.title), ['Warp Films', 'Warp Records']);
  assert.deepEqual(shown[0], ['Warp Films'], "MusicBrainz' rows before Discogs answered");
});

test('a search whose job fails says its words', async () => {
  server({ [GET]: [() => json(404, {})], [POST]: [() => json(202, { task: task(32, null, null) })] });
  const got = searchBeyond('macadam');
  await settle();
  emit('task', task(32, 3, null, 'The search could not be asked.'));
  await assert.rejects(got, /The search could not be asked\./);
});
