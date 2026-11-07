// Cratarr's reach settling before reads move (lib/deviceIndex, the real one, on lib/reach): a failed request is looked at again
// seconds later, and two failed looks move the reads. What a test asks is where reads come from, and who was told.
import { afterEach, beforeEach, mock, test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const { networkSaid, reach, reached, refused, unreached } = await import('../../src/lib/reach.svelte.ts');
const { readsNow, whenReadsMove } = await import('../../src/lib/deviceIndex.svelte.ts');
const { LOOK_AGAIN_MS } = await import('../../src/lib/deviceIndexRule.ts');

/** What /api/health answers; null: the network fails. */
let health: number | null = null;
let looks = 0;
globalThis.fetch = (async () => {
  looks++;
  if (health === null) throw new TypeError('Failed to fetch');
  return new Response('{}', { status: health });
}) as typeof fetch;

let moves = 0;
whenReadsMove.add(() => moves++);

/** `ms` go by, and what they set going ends. */
async function after(ms: number) {
  mock.timers.tick(ms);
  for (let i = 0; i < 5; i++) await new Promise((go) => setImmediate(go));
}

beforeEach(() => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 1_000_000 });
  [moves, looks, health] = [0, 0, null];
});
// Reached first: it clears the look under way, and a timer from before a reset is one the mock cannot clear.
afterEach(() => {
  reached();
  mock.timers.reset();
});

test('a failed request is out of reach at once, and reads stay on the server until it is looked at again', async () => {
  unreached();
  assert.equal(reach.away, true);
  assert.deepEqual(readsNow(), { from: 'server', why: 'answers' });
  await after(LOOK_AGAIN_MS - 1);
  assert.equal(looks, 0);
  assert.equal(moves, 0);
});

test('a second look that fails moves the reads to the device, and says so once', async () => {
  unreached();
  await after(LOOK_AGAIN_MS);
  assert.equal(looks, 1);
  assert.deepEqual(readsNow(), { from: 'device', why: 'unreachable' });
  assert.equal(moves, 1);
  unreached();
  await after(LOOK_AGAIN_MS);
  assert.equal(looks, 1, 'settled, nothing is looked at again here');
  assert.equal(moves, 1);
});

test('many failed requests are one look', async () => {
  unreached();
  await after(1000);
  unreached();
  unreached();
  await after(LOOK_AGAIN_MS);
  assert.equal(looks, 1);
});

test('a second look that finds Cratarr was a blip: no page moved, and it counts as reached', async () => {
  health = 200;
  unreached();
  await after(LOOK_AGAIN_MS);
  assert.equal(moves, 0);
  assert.deepEqual(readsNow(), { from: 'server', why: 'answers' });
  assert.equal(reach.away, false);
});

test('a proxy answering for a stopped Cratarr is not Cratarr', async () => {
  health = 502;
  unreached();
  await after(LOOK_AGAIN_MS);
  assert.deepEqual(readsNow(), { from: 'device', why: 'unreachable' });
});

test('Cratarr answering moves them back at once, and the look under way is dropped', async () => {
  unreached();
  await after(LOOK_AGAIN_MS);
  const told = moves;
  reached();
  assert.deepEqual(readsNow(), { from: 'server', why: 'answers' });
  assert.equal(moves, told + 1);
  unreached();
  reached();
  await after(LOOK_AGAIN_MS);
  assert.equal(looks, 1, 'the one before: answered first, this one never went');
});

test('no network moves the reads with no look at all, and the network back moves them back, each told', async () => {
  networkSaid(false);
  assert.deepEqual(readsNow(), { from: 'device', why: 'no-network' });
  assert.equal(moves, 1);
  networkSaid(true);
  assert.deepEqual(readsNow(), { from: 'server', why: 'answers' });
  assert.equal(moves, 2);
  assert.equal(looks, 0);
});

test('a refusal holding the requests back holds the look too: no more knocks, and it counts as not found', async () => {
  refused();
  unreached();
  await after(LOOK_AGAIN_MS);
  assert.equal(looks, 0);
  assert.deepEqual(readsNow(), { from: 'device', why: 'unreachable' });
  await after(6 * 60_000);
});
