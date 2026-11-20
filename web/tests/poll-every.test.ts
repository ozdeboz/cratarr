// RC01: the status poll can never stop (0.16.2 awaited each answer before asking again, so one request that never ended stopped it,
// and the desktop app said "unreachable" for hours). lib/pollEvery on node:test's mock timers; App's status poll by its source.
// Red first: App.svelte at 7da786c91 chains `setTimeout(poll, 30_000)` after its await.
import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pollEvery, type Polled } from '../src/lib/pollEvery.ts';

/** A promise and its ends. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const p = new Promise<T>((res, rej) => ((resolve = res), (reject = rej)));
  return { p, resolve, reject };
}
const settled = () => new Promise((r) => setImmediate(r));

test('an ask that never ends does not stop the poll: the next tick asks again', () => {
  mock.timers.enable({ apis: ['setInterval'] });
  try {
    let asks = 0;
    const stop = pollEvery(30_000, () => (asks++, new Promise<never>(() => {})), () => {});
    assert.equal(asks, 1, 'at once');
    mock.timers.tick(30_000);
    assert.equal(asks, 2);
    mock.timers.tick(60_000);
    assert.equal(asks, 4);
    stop();
    mock.timers.tick(60_000);
    assert.equal(asks, 4, 'stopped');
  } finally {
    mock.timers.reset();
  }
});

test('a failure is heard, and the poll goes on after it', async () => {
  mock.timers.enable({ apis: ['setInterval'] });
  try {
    const heard: Polled<number>[] = [];
    let n = 0;
    const stop = pollEvery(1_000, () => (++n === 1 ? Promise.reject(new Error('away')) : Promise.resolve(n)), (r) => heard.push(r));
    await settled();
    mock.timers.tick(1_000);
    await settled();
    assert.equal(heard.length, 2);
    assert.equal(heard[0].ok, false);
    assert.deepEqual(heard[1], { ok: true, value: 2 });
    stop();
  } finally {
    mock.timers.reset();
  }
});

test('answers are taken in the order asked: one older than one already taken is dropped, and none once stopped', async () => {
  mock.timers.enable({ apis: ['setInterval'] });
  try {
    const asks = [deferred<string>(), deferred<string>(), deferred<string>()];
    let i = 0;
    const heard: Polled<string>[] = [];
    const stop = pollEvery(1_000, () => asks[i++].p, (r) => heard.push(r));
    mock.timers.tick(1_000);
    asks[1].resolve('second');
    await settled();
    asks[0].reject(new Error('first, let go late'));
    await settled();
    assert.deepEqual(heard, [{ ok: true, value: 'second' }], 'the late failure of the first says nothing over the second');
    mock.timers.tick(1_000);
    stop();
    asks[2].resolve('third');
    await settled();
    assert.equal(heard.length, 1);
  } finally {
    mock.timers.reset();
  }
});

test("App's status line is polled on a timer, never by a chain that awaits; busy is not away", () => {
  const app = readFileSync(new URL('../src/App.svelte', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
  assert.ok(app.includes('return pollEvery(30_000, getStatus, (r) => {'));
  assert.ok(!app.includes('setTimeout(poll, 30_000)'), 'no chain of its own');
  assert.ok(app.includes('if (!tookTooLong(r.error)) statusFailed = true;'), 'a request let go as busy leaves the line as it was');
});
