// A cover that failed while Cratarr was out of reach is asked again when it is back (0.16.0 on the tablet, 2026-11-08: every
// cover asked with Wi-Fi off stayed a stand-in once it was back, until the page was opened again). Only a real "no cover"
// answer, a 404, is remembered (the review of 1.2, M4: a server error or a refused cover under load stayed a stand-in on an
// owned album until a reload); any other failure is asked again after a pause (lib/coverFailures and the two places that keep
// failures, the real ones).
import { test, mock, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { failures, channel, foldAnswers, GRACE_MS, NO_COVER, AGAIN_AFTER } = await import('../../src/lib/coverFailures.svelte.ts');

afterEach(() => mock.timers.reset());

test('what failed while out of reach, or just before it was seen, is asked again once back; a real "no cover" before is not', () => {
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
  at(t0, () => misses.note('/no-art', NO_COVER));
  at(t0 + 100_000 - GRACE_MS + 1, () => misses.note('/in-flight', NO_COVER));
  channel(false, t0 + 100_000);
  at(t0 + 101_000, () => misses.note('/away', null));
  at(t0 + 102_000, () => also.note('/player-away', null));
  assert.deepEqual(Object.keys(misses.failed).sort(), ['/away', '/in-flight', '/no-art'], 'out of reach, the stand-in still');
  channel(false, t0 + 150_000);
  channel(true, t0 + 200_000);
  assert.deepEqual(Object.keys(misses.failed), ['/no-art'], 'a cover that answered "no cover" with Cratarr there has none');
  assert.deepEqual(Object.keys(also.failed), [], "the player's are asked again too");
  channel(true, t0 + 300_000);
  assert.deepEqual(Object.keys(misses.failed), ['/no-art'], 'connected again, nothing more forgotten');
});

test('only a 404 is remembered; a server error, a network failure or one the browser does not explain is asked again after a pause', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 5_000_000 });
  const misses = failures();
  misses.note('/none', NO_COVER);
  misses.note('/busy', 503);
  misses.note('/failed', 500);
  misses.note('/lost', 0);
  misses.note('/unknown', null);
  assert.deepEqual(Object.keys(misses.failed).sort(), ['/busy', '/failed', '/lost', '/none', '/unknown'], 'the stand-in at once, all of them');
  mock.timers.tick(AGAIN_AFTER[0] - 1);
  assert.equal(Object.keys(misses.failed).length, 5, 'not before its pause');
  mock.timers.tick(1);
  assert.deepEqual(Object.keys(misses.failed), ['/none'], 'asked again after a few seconds: only "no cover" stays');
  mock.timers.tick(24 * 3600_000);
  assert.deepEqual(Object.keys(misses.failed), ['/none'], 'and stays');
});

test('an address that fails again waits longer each time, never for good; a 404 later is kept', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 5_000_000 });
  const misses = failures();
  const waits: number[] = [];
  for (let i = 0; i < AGAIN_AFTER.length + 2; i++) {
    misses.note('/busy', 503);
    let waited = 0;
    while (misses.failed['/busy']) {
      mock.timers.tick(1000);
      waited += 1000;
      assert.ok(waited <= AGAIN_AFTER[AGAIN_AFTER.length - 1], 'never kept for good');
    }
    waits.push(waited);
  }
  assert.deepEqual(waits, [...AGAIN_AFTER, AGAIN_AFTER[AGAIN_AFTER.length - 1], AGAIN_AFTER[AGAIN_AFTER.length - 1]]);
  assert.ok(AGAIN_AFTER[0] <= 10_000, 'the first again within a few seconds');
  misses.note('/busy', NO_COVER);
  mock.timers.tick(24 * 3600_000);
  assert.ok(misses.failed['/busy'], 'answered "no cover": kept');
});

test('many places failing at once on one address make one pause, not one each; a 404 among them keeps it', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 5_000_000 });
  const misses = failures();
  for (let i = 0; i < 12; i++) misses.note('/shelf', 503);
  mock.timers.tick(AGAIN_AFTER[0]);
  assert.ok(!misses.failed['/shelf'], 'asked again after the first pause, not the twelfth');
  misses.note('/sized', 503);
  misses.note('/sized', NO_COVER);
  mock.timers.tick(AGAIN_AFTER[0]);
  assert.ok(misses.failed['/sized'], 'a real "no cover" outranks a failure before it');
});

test("the browser's resource timing says what a failed address answered: the last answer counts, a success or no status forgets it", () => {
  const seen = new Map<string, number>();
  const entry = (name: string, responseStatus: number) => ({ name, responseStatus }) as PerformanceResourceTiming;
  foldAnswers(seen, [entry('http://cratarr.test/a', 404), entry('http://cratarr.test/b', 503), entry('http://cratarr.test/c', 404)]);
  foldAnswers(seen, [entry('http://cratarr.test/b', 200), entry('http://cratarr.test/c', 0)]);
  assert.deepEqual([...seen], [['http://cratarr.test/a', 404]], 'a later success, or one with no status (a network failure), is no "no cover"');
  foldAnswers(seen, [entry('http://cratarr.test/a', 503)]);
  assert.deepEqual([...seen], [['http://cratarr.test/a', 503]]);
});

test('the cover and the player keep their failures here, so both are asked again; the cover notes what its address answered', async () => {
  const { readFileSync } = await import('node:fs');
  const read = (p: string) => readFileSync(new URL(p, import.meta.url), 'utf8');
  const cover = read('../../src/lib/Cover.svelte');
  assert.match(cover, /const misses = failures\(\);[\s\S]*misses\.note\(src, why\)/);
  assert.match(cover, /const why = answered\(at\);\s*if \(src\) coverBroke\(src, why\);\s*coverBroke\(at, why\);/);
  assert.match(read('../../src/lib/remote.svelte.ts'), /const misses = failures\(\);[\s\S]*coverFailed = \(src: string\) => misses\.note\(src\)/);
});
