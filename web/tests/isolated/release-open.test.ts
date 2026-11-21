// Replace's release (SCENARIOS-LOOKUPS LK.B2, its page half): what Cratarr keeps is read; where none is kept yet, the
// release is opened (`POST …/release`), whose answer is the kept release or its job's task, followed by its id until it
// ends, then read. It works with today's answer (a 200 from the read) and with the server's half (404, then 202 and a task).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { getRelease } = await import('../../src/lib/replaceTrack.ts');
const { emit } = await import('../../src/lib/events.svelte.ts');
const { tasks } = await import('../../src/lib/tasks.svelte.ts');

const release = (id: string) => ({ release: id, title: 'Night Ferry', artist: 'Lowland', year: 1999, tracks: [] });
const task = (id: number, finished: number | null = null, error: string | null = null) => ({
  id,
  kind: 'release',
  preview: false,
  started: 1,
  finished,
  step: null,
  error,
  result: null,
});
const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

/** The server stood in for: each answer in turn for its method and path, every ask kept. */
function server(answers: Record<string, (() => Response)[]>) {
  const asked: string[] = [];
  (globalThis as { fetch: unknown }).fetch = async (path: string, init?: { method?: string }) => {
    const key = `${init?.method ?? 'GET'} ${path}`;
    asked.push(key);
    if (path === '/api/tasks') return json(200, { tasks: [] });
    const next = answers[key]?.shift();
    if (!next) throw new Error(`nothing stood in for ${key}`);
    return next();
  };
  return asked;
}
const PATH = '/api/albums/cratarr/al-42/release';
const settle = () => new Promise((r) => setTimeout(r, 0));

test('a kept release is read, and nothing is opened', async () => {
  const asked = server({ [`GET ${PATH}`]: [() => json(200, release('R1'))] });
  assert.equal((await getRelease('cratarr', 'al-42')).release, 'R1');
  assert.deepEqual(asked, [`GET ${PATH}`]);
});

test('none kept: the release is opened, its task followed until it ends, then read', async () => {
  const asked = server({
    [`GET ${PATH}`]: [() => json(404, { error: 'Its release is not read yet.' }), () => json(200, release('R1'))],
    [`POST ${PATH}`]: [() => json(202, { task: task(7) })],
  });
  const got = getRelease('cratarr', 'al-42');
  await settle();
  await settle();
  assert.deepEqual(asked.filter((a) => a !== 'GET /api/tasks'), [`GET ${PATH}`, `POST ${PATH}`], 'opened, then waiting on its task');
  emit('task', task(7, 2));
  assert.equal((await got).release, 'R1');
  assert.deepEqual(asked.filter((a) => a !== 'GET /api/tasks'), [`GET ${PATH}`, `POST ${PATH}`, `GET ${PATH}`]);
});

test('an open that answers the kept release shows it at once', async () => {
  const asked = server({
    [`GET ${PATH}`]: [() => json(404, { error: 'Its release is not read yet.' })],
    [`POST ${PATH}`]: [() => json(200, release('R2'))],
  });
  assert.equal((await getRelease('cratarr', 'al-42')).release, 'R2');
  assert.deepEqual(asked, [`GET ${PATH}`, `POST ${PATH}`]);
});

test('a job that fails says its words', async () => {
  server({
    [`GET ${PATH}`]: [() => json(404, { error: 'Its release is not read yet.' })],
    [`POST ${PATH}`]: [() => json(202, { task: task(8) })],
  });
  const got = getRelease('cratarr', 'al-42');
  await settle();
  await settle();
  emit('task', task(8, 2, 'MusicBrainz did not answer.'));
  await assert.rejects(got, /MusicBrainz did not answer\./);
});

test("two albums' jobs of one kind are each followed by their own id", async () => {
  const other = '/api/albums/cratarr/al-43/release';
  server({
    [`GET ${PATH}`]: [() => json(404, {}), () => json(200, release('R1'))],
    [`POST ${PATH}`]: [() => json(202, { task: task(11) })],
    [`GET ${other}`]: [() => json(404, {}), () => json(200, release('R3'))],
    [`POST ${other}`]: [() => json(202, { task: task(12) })],
  });
  let first = false;
  const a = getRelease('cratarr', 'al-42').then((r) => ((first = true), r));
  const b = getRelease('cratarr', 'al-43');
  await settle();
  await settle();
  emit('task', task(12, 3));
  assert.equal((await b).release, 'R3');
  assert.equal(first, false, "the other album's end never ends this one");
  emit('task', task(11, 4));
  assert.equal((await a).release, 'R1');
  assert.equal(tasks.running('release'), false);
});
