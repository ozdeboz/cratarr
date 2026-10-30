// One request of a kind on its way at a time (lib/oneInFlight): the state reports that piled up on 2026-10-30 and held
// a hold's stream back from the server.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { oneInFlight } = await import('../src/lib/oneInFlight.ts');

/** A send that waits until let go, counting how many are on their way at once. */
function gate() {
  const sent: boolean[] = [];
  const waiting: (() => void)[] = [];
  let open = 0;
  let most = 0;
  const send = (a: boolean) => {
    sent.push(a);
    open++;
    most = Math.max(most, open);
    return new Promise<void>((done) => waiting.push(() => (open--, done())));
  };
  const letGo = async () => {
    waiting.shift()?.();
    // The finished one's follow-up starts on the next turns.
    for (let i = 0; i < 5; i++) await Promise.resolve();
  };
  return { sent, send, letGo, most: () => most };
}

test('never two on their way: asked while one is, it waits for it', async () => {
  const g = gate();
  const report = oneInFlight(g.send, (a, b) => a || b);
  void report(false);
  void report(false);
  void report(false);
  assert.deepEqual(g.sent, [false], 'only the first went');
  await g.letGo();
  assert.deepEqual(g.sent, [false, false], 'once more after it, not three times');
  await g.letGo();
  assert.deepEqual(g.sent, [false, false], 'nothing asked meanwhile: nothing more');
  assert.equal(g.most(), 1);
});

test('what was asked meanwhile is merged into the one after', async () => {
  const g = gate();
  const report = oneInFlight(g.send, (a, b) => a || b);
  void report(false);
  void report(true);
  void report(false);
  await g.letGo();
  assert.deepEqual(g.sent, [false, true], 'the queue asked for meanwhile is sent');
});

test('a send that fails does not stop the next', async () => {
  let calls = 0;
  const report = oneInFlight(async () => {
    calls++;
    throw new Error('the server did not answer');
  }, (_a: number, b: number) => b);
  await report(1);
  await report(2);
  assert.equal(calls, 2);
});
