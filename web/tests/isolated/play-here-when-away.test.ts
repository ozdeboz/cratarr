// With Cratarr out of reach a player of its own cannot be reached either, so the music goes to this device for the moment,
// and the choice stays as it was (the owner, 2026-11-08: "Play here instead"; 0.16.0 pressed play on the tablet for the
// ThinkPad, and nothing played). lib/outputNow and lib/playOn, the real ones.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const { prefs } = await import('../../src/lib/prefs.svelte.ts');
const { reach } = await import('../../src/lib/reach.svelte.ts');
const { outputNow } = await import('../../src/lib/outputNow.svelte.ts');

test('away, a player chosen plays here for the moment; back, the choice is used as before and was never changed', () => {
  prefs.output = 'player:ThinkPad';
  reach.away = false;
  assert.equal(outputNow(), 'player:ThinkPad');
  reach.away = true;
  assert.equal(outputNow(), 'local', 'Cratarr out of reach: here');
  assert.equal(prefs.output, 'player:ThinkPad', 'the choice stays');
  reach.away = false;
  assert.equal(outputNow(), 'player:ThinkPad', 'back: as chosen');
  prefs.output = 'local';
  reach.away = true;
  assert.equal(outputNow(), 'local');
});

test('where the music is routed reads the moment, not the choice', () => {
  const read = (p: string) => readFileSync(new URL(`../../src/lib/${p}`, import.meta.url), 'utf8');
  assert.match(read('player/port.svelte.ts'), /holdsQueue\(outputNow\(\), canPlayHere, drivenName\(\)\)/, 'which side holds the queue');
  assert.match(read('playOn.svelte.ts'), /const out = outputNow\(\);\s*return out\.startsWith\('player:'\)/, 'the player driven');
  assert.match(read('output.svelte.ts'), /playsHere = \(\) => outputNow\(\) === 'local'/);
  assert.match(read('localPlayer.svelte.ts'), /handed\(\) \|\| outputNow\(\) !== 'local'/, 'the kept queue taken up away');
});
