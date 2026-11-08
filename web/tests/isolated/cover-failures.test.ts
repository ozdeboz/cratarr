// A cover that failed while Cratarr was out of reach is asked again when it is back (0.16.0 on the tablet, 2026-11-08: every
// cover asked with Wi-Fi off stayed a stand-in once it was back, until the page was opened again); one that failed with
// Cratarr there has none, and stays the stand-in (lib/coverFailures and the two places that keep failures, the real ones).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { failures, channel, GRACE_MS } = await import('../../src/lib/coverFailures.svelte.ts');

test('what failed while out of reach, or just before it was seen, is asked again once back; what failed before is not', () => {
  const t0 = 1_000_000;
  const misses = failures();
  const also = failures();
  channel(true, t0);
  const realNow = Date.now;
  const at = (ms: number, f: () => void) => {
    Date.now = () => ms;
    try {
      f();
    } finally {
      Date.now = realNow;
    }
  };
  at(t0, () => misses.note('/no-art'));
  at(t0 + 100_000 - GRACE_MS + 1, () => misses.note('/in-flight'));
  channel(false, t0 + 100_000);
  at(t0 + 101_000, () => misses.note('/away'));
  at(t0 + 102_000, () => also.note('/player-away'));
  assert.deepEqual(Object.keys(misses.failed).sort(), ['/away', '/in-flight', '/no-art'], 'out of reach, the stand-in still');
  channel(false, t0 + 150_000);
  channel(true, t0 + 200_000);
  assert.deepEqual(Object.keys(misses.failed), ['/no-art'], 'a cover that failed with Cratarr there has none');
  assert.deepEqual(Object.keys(also.failed), [], "the player's are asked again too");
  channel(true, t0 + 300_000);
  assert.deepEqual(Object.keys(misses.failed), ['/no-art'], 'connected again, nothing more forgotten');
});

test('the cover and the player keep their failures here, so both are asked again', async () => {
  const { readFileSync } = await import('node:fs');
  const read = (p: string) => readFileSync(new URL(p, import.meta.url), 'utf8');
  assert.match(read('../../src/lib/Cover.svelte'), /const misses = failures\(\);[\s\S]*misses\.note\(src\)/);
  assert.match(read('../../src/lib/remote.svelte.ts'), /const misses = failures\(\);[\s\S]*coverFailed = \(src: string\) => misses\.note\(src\)/);
});
