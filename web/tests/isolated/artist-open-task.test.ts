// An artist opened by Beyond's id (SCENARIOS-LOOKUPS LK.P1, its page half): the open answers at once with the name's one
// artist, and the page shows them; where it also answers a task (MusicBrainz' question, a job), the page follows it by its
// id and moves to the artist its result names where that differs, as a page moves to an id the server names. It works with
// today's answer (no task) and the server's half (a task beside the artist).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { openArtistPage } = await import('../../src/lib/albums.svelte.ts');
const { panel, artistSubject } = await import('../../src/lib/panel.svelte.ts');
const { emit } = await import('../../src/lib/events.svelte.ts');

const task = (id: number, finished: number | null = null, result: unknown = null, error: string | null = null) => ({
  id,
  kind: 'artist:open',
  preview: false,
  started: 1,
  finished,
  step: null,
  error,
  result,
});
const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
function server(open: unknown) {
  (globalThis as { fetch: unknown }).fetch = async (path: string) => {
    if (path === '/api/artists/open') return json(200, open);
    if (path === '/api/tasks') return json(200, { tasks: [] });
    return json(200, {});
  };
}
const settle = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};
const viewing = (id: string) => artistSubject('cratarr', id);

test("today's answer: the named artist's page, nothing followed", async () => {
  server({ artist: 'ar-361' });
  await openArtistPage({ mbid: 'mb-sigur', name: 'Sigur Rós' });
  assert.equal(panel.viewed, viewing('ar-361'));
});

test("a task whose result names the same artist leaves the page where it is", async () => {
  server({ artist: 'ar-391', task: task(21) });
  await openArtistPage({ mbid: 'mb-ph', name: 'Low Harbour' });
  assert.equal(panel.viewed, viewing('ar-391'), 'shown at once, before the job ends');
  emit('task', task(21, 2, { artist: 'ar-391' }));
  await settle();
  assert.equal(panel.viewed, viewing('ar-391'));
});

test('a task whose result names another artist moves the page there', async () => {
  server({ artist: 'ar-394', task: task(22) });
  await openArtistPage({ mbid: 'mb-ph2', name: 'Pale Hours' });
  assert.equal(panel.viewed, viewing('ar-394'));
  emit('task', task(22, 3, { artist: 'ar-399' }));
  await settle();
  assert.equal(panel.viewed, viewing('ar-399'));
});

test('a task that fails, or names no artist, leaves the page on the artist answered', async () => {
  server({ artist: 'ar-393', task: task(23) });
  await openArtistPage({ mbid: 'mb-x', name: 'Quiet Rooms' });
  emit('task', task(23, 4, null, 'MusicBrainz did not answer.'));
  await settle();
  assert.equal(panel.viewed, viewing('ar-393'));
});
