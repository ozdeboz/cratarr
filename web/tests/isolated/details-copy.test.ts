// The player's Details say the copy that streams (the owner, testing 0.16.0: Quality and File described the main copy while Play from
// played another): the details are asked for the copy the queue names, as the stream is.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const asked: string[] = [];
globalThis.fetch = (async (input: string) => {
  asked.push(String(input));
  return new Response('{}', { headers: { 'content-type': 'application/json' } });
}) as typeof fetch;
const { trackFile } = await import('../../src/lib/api.ts');
const source = (name: string) => readFileSync(new URL(`../../src/lib/${name}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test("a track's details are asked of the copy named, and of the one that would play where none is", async () => {
  await trackFile('cratarr', 'tr-1', 'cratarr/second');
  await trackFile('cratarr', 'tr-1');
  assert.deepEqual(asked.filter((a) => a.includes('/details')), ['/api/track/cratarr/tr-1/details?from=cratarr%2Fsecond', '/api/track/cratarr/tr-1/details']);
});

test("the playing track's view carries its queue's copy, and its menu asks the details of that copy", () => {
  assert.match(source('player/port.svelte.ts'), /\.\.\.\(t\.from \? \{ from: t\.from \} : \{\}\)/);
  assert.match(source('TrackMenu.svelte'), /trackFile\(source, id, from \|\| undefined\)/);
});
