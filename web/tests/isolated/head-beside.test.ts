// The tape head stands beside the old player (decision 0102: moving normal play onto it is its gate, the owner's call). The real
// player on tests/harness/fakeStream, every `$effect` run by hand: a track played in a browser asks nothing of the head.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { effects } from '../harness/pageGlobals.ts';

const { audio, fromRest, player, runFor, server, settle, track } = await import('../harness/fakeStream.ts');
const { local, playQueue } = player;

const asked: string[] = [];
const streams = globalThis.fetch;
globalThis.fetch = ((input: string, init?: RequestInit) => {
  asked.push(String(input));
  return streams(input, init);
}) as typeof fetch;

test('a track played here is heard from its stream, and no whole file of it is fetched for the head', async () => {
  await fromRest();
  playQueue([track('a', 100)], 0);
  await settle();
  for (const run of effects) run();
  await runFor(1);
  assert.equal(audio.heard()?.track, 'a');
  assert.equal(local.paused, false);
  assert.deepEqual(server.asked, [{ track: 'a', from: 0 }]);
  assert.deepEqual(asked.filter((a) => a.startsWith('/api/file/')), []);
});
