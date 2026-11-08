// A page opened on a queue that went on while Cratarr was out of reach (decision 0101, point 19; lib/localPlayer, the real one):
// Cratarr holds an older one, so what this device kept is taken up as the page opens, before anything has been asked.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

// `window`: lib/account reads the name kept only in a page, and with it what else a page finds.
const g = globalThis as Record<string, unknown>;
g.window = globalThis;
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };

const copy = 'cratarr//incoming';
const song = (id: string, from?: string) => ({ source: 'cratarr', id, name: id, artist: null, album: null, album_id: null, duration_secs: 200, ...(from ? { from } : {}) });
localStorage.setItem('cratarr-who', '"ann"');
localStorage.setItem(
  'cratarr-queue-now@ann',
  JSON.stringify({ id: 'q-away', name: 'On the road', renamed: true, updated: 1, tracks: [song('a', copy), song('b', copy)], index: 1, position: 42, playing: true, shuffled: false, repeat: 'off', radio: false, radioBy: 'track', radioCount: 5, skipFavourites: false, autoplay: null, ahead: true }),
);
// Hers, and bob's beside it: never read for her.
localStorage.setItem('cratarr-queue-now@bob', JSON.stringify({ id: 'q-bob', tracks: [song('x')], index: 0, position: 0, ahead: true }));

const { audio, player, server, settle } = await import('../harness/fakeStream.ts');
const { keptHere, local } = player;
server.lengths.set('a', 200);
server.lengths.set('b', 200);

test('it opens on the queue kept, on the track and at the place it reached, paused', async () => {
  await settle();
  assert.deepEqual(local.queue.map((t) => t.id), ['a', 'b']);
  assert.equal(local.index, 1);
  assert.equal(local.position, 42);
  assert.equal(local.paused, true);
  assert.equal(audio.paused, true, 'a page starts no sound of its own');
  assert.equal(local.from, 'On the road');
});

test('it is the queue that plays, under its own id, with its name kept', () => {
  assert.equal(keptHere.id, 'q-away');
  assert.equal(keptHere.renamed, true);
  assert.deepEqual(keptHere.list, []);
});

test('its tracks keep the copy they play from, and its stream asks for it', () => {
  assert.deepEqual(local.queue.map((t) => t.from), [copy, copy]);
  const asked = new URL(server.addresses[0], 'http://cratarr.test');
  assert.equal(asked.pathname, '/api/stream/cratarr/b');
  assert.equal(asked.searchParams.get('from'), copy);
  assert.equal(asked.searchParams.get('from_ms'), '42000');
});
