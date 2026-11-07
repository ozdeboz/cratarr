// The queue that plays, kept on this device (decision 0101, points 14 and 19; lib/localPlayer, the real one, on
// tests/harness/fakeStream): each account's own, at the place it reached, taken up where Cratarr cannot give it back.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { audio, fromRest, player, runFor, server, settle, track } from '../harness/fakeStream.ts';

const { emptyHere, enqueue, jump, keptHere, local, next, playOnCommand, playQueue, queueTold, stop, togglePlay } = player;
const { accountForgotten, accountLearnt, perAccount } = await import('../../src/lib/account.ts');
const { reach, unreached } = await import('../../src/lib/reach.svelte.ts');

const near = (a: number, b: number, what: string) => assert.ok(Math.abs(a - b) < 1, `${what}: ${a}, not about ${b}`);
/** What this device keeps for `who` now. */
const kept = (who = 'ann') => JSON.parse(localStorage.getItem(`cratarr-queue-now@${who}`) ?? 'null');
const ids = (tracks: { id: string }[]) => tracks.map((t) => t.id);

beforeEach(async () => {
  reach.away = false;
  accountLearnt('ann');
  await fromRest();
});

/** `tracks` from the first, and a moment of it. */
async function plays(...tracks: ReturnType<typeof track>[]) {
  playQueue(tracks);
  await settle();
  await runFor(1);
}

/** The page as it opens with nothing playing; `unreached` says whether Cratarr is there. The stream the test played stays
 *  the module's: only what the player holds is gone, as nothing else here reads it. */
const nothingHeld = () => emptyHere();

test('the queue that plays is kept under its account, its place going on with it', async () => {
  await plays(track('a', 200), track('b', 200));
  const first = kept();
  assert.deepEqual(ids(first.tracks), ['a', 'b']);
  assert.equal(first.index, 0);
  assert.equal(first.playing, true);
  assert.equal(first.ahead, false, 'Cratarr is there');
  await runFor(30);
  assert.ok(kept().position > 20, 'its place, no further behind than the time between two keepings');
  assert.equal(localStorage.getItem('cratarr-queue-now@bob'), null, 'nobody else has it');
  assert.equal(localStorage.getItem('cratarr-queue-now'), null);
});

test('a change of queue or of track is kept at the clock\'s next moment, with no wait for the place\'s turn', async () => {
  await plays(track('a', 200), track('b', 200));
  enqueue([track('c', 100)], 'next');
  await runFor(0.25);
  assert.deepEqual(ids(kept().tracks), ['a', 'c', 'b']);
  jump(2);
  await settle();
  await runFor(0.25);
  assert.equal(kept().index, 2);
});

test('paused, it says so and writes no more', async () => {
  await plays(track('a', 200));
  togglePlay();
  await settle();
  await runFor(0.25);
  assert.equal(kept().playing, false);
  localStorage.setItem('cratarr-queue-now@ann', JSON.stringify({ ...kept(), mark: true }));
  await runFor(30);
  assert.equal(kept().mark, true, 'not written again');
});

test('a queue that leaves this page leaves nothing behind', async () => {
  await plays(track('a', 200));
  assert.ok(kept());
  stop();
  await settle();
  assert.equal(kept(), null);
});

test('a queue Cratarr sends takes the place of the one kept', async () => {
  await plays(track('a', 200));
  playOnCommand({ command: 'play', tracks: [track('z', 100)], index: 0, queue_id: 'q1', queue_name: 'Theirs' });
  await settle();
  await runFor(0.25);
  assert.deepEqual(ids(kept().tracks), ['z']);
  assert.equal(kept().id, 'q1');
  assert.equal(kept().name, 'Theirs');
});

test('Cratarr out of reach: a player with nothing takes up what is kept here, paused at the place it reached', async () => {
  await plays(track('a', 200), track('b', 200));
  next();
  await settle();
  await runFor(40);
  assert.equal(local.queue[local.index].id, 'b');
  nothingHeld();
  server.asked.length = 0;
  unreached();
  await settle();
  assert.deepEqual(ids(local.queue), ['a', 'b']);
  assert.equal(local.index, 1, 'on the track it reached');
  assert.ok(local.position > 30, 'at its place, no further behind than the time between two keepings');
  assert.equal(local.paused, true);
  assert.equal(audio.paused, true, 'no sound of its own accord');
  assert.ok(server.asked.every((a) => a.track === 'b'), 'its stream ready from there');
});

test('its settings come with it', async () => {
  await plays(track('a', 200));
  Object.assign(local, { repeat: 'all', radio: true, radioBy: 'era', radioCount: 3, skipFavourites: true, from: 'Chill' });
  await runFor(10);
  Object.assign(local, { repeat: 'off', radio: false, radioBy: 'track', radioCount: 5, skipFavourites: false, from: null });
  nothingHeld();
  unreached();
  await settle();
  assert.deepEqual({ r: local.repeat, on: local.radio, by: local.radioBy, n: local.radioCount, skip: local.skipFavourites, from: local.from }, { r: 'all', on: true, by: 'era', n: 3, skip: true, from: 'Chill' });
  Object.assign(local, { repeat: 'off', radio: false, skipFavourites: false });
});

test('a Play from queue keeps the copy it plays from: kept, and asked for from it once taken up', async () => {
  const copy = 'cratarr//incoming';
  playQueue([{ ...track('a', 200), from: copy }, track('b', 200)]);
  await settle();
  await runFor(1);
  assert.equal(kept().tracks[0].from, copy);
  assert.equal(kept().tracks[1].from, undefined, 'and a track that names none names none');
  nothingHeld();
  server.addresses.length = 0;
  unreached();
  await settle();
  assert.equal(local.queue[0].from, copy);
  assert.equal(new URL(server.addresses[0], 'http://cratarr.test').searchParams.get('from'), copy);
});

test('what is held is not replaced by what is kept', async () => {
  await plays(track('a', 200));
  localStorage.setItem('cratarr-queue-now@ann', JSON.stringify({ ...kept(), tracks: [track('old', 100)] }));
  unreached();
  await settle();
  assert.deepEqual(ids(local.queue), ['a']);
});

test('a copy Cratarr has been told of waits for Cratarr to give it back, with Cratarr there', async () => {
  await plays(track('a', 200));
  nothingHeld();
  accountLearnt('bob');
  accountLearnt('ann');
  await settle();
  assert.equal(local.queue.length, 0, 'Cratarr gives its own back, with its word on whether it plays on');
  assert.ok(kept(), 'and the copy stands');
});

test('away, the queue is ahead of Cratarr until a report has reached it', async () => {
  await plays(track('a', 200));
  unreached();
  await runFor(0.25);
  jump(0);
  await settle();
  await runFor(11);
  assert.equal(kept().ahead, true, 'Cratarr has not heard how it stands');
  reach.away = false;
  await runFor(11);
  assert.equal(kept().ahead, true, 'Cratarr answering is not Cratarr told');
  queueTold();
  assert.equal(kept().ahead, false);
});

test('a queue ahead of Cratarr is taken up with Cratarr there too: Cratarr has an older one', async () => {
  await plays(track('a', 200));
  unreached();
  await runFor(11);
  assert.equal(kept().ahead, true);
  reach.away = false;
  nothingHeld();
  accountLearnt('bob');
  accountLearnt('ann');
  await settle();
  assert.deepEqual(ids(local.queue), ['a']);
  assert.equal(local.paused, true);
});

test('another account signing in: what plays goes with the one who left, and shows to nobody else', async () => {
  await plays(track('a', 200), track('b', 200));
  await runFor(5);
  accountLearnt('bob');
  await settle();
  assert.equal(local.queue.length, 0, 'not left in the player');
  assert.equal(audio.paused, true, 'nor heard');
  await runFor(11);
  assert.equal(localStorage.getItem('cratarr-queue-now@bob'), null, 'nothing kept under him');
  assert.deepEqual(ids(kept().tracks), ['a', 'b'], 'hers as it stood');
  near(kept().position, 6, 'at her place');
});

test('what she changed last, paused, leaves with her too: no clock came after it', async () => {
  await plays(track('a', 200));
  togglePlay();
  await settle();
  enqueue([track('c', 100)], 'next');
  accountLearnt('bob');
  await settle();
  assert.deepEqual(ids(kept().tracks), ['a', 'c']);
});

test('nobody signed in takes up nothing, even where something lies under nobody', async () => {
  localStorage.setItem('cratarr-queue-now-', JSON.stringify({ ...JSON.parse('{"id":"x","name":"x","index":0,"position":0,"ahead":true}'), tracks: [track('z', 100)] }));
  accountForgotten();
  await settle();
  unreached();
  await settle();
  assert.equal(local.queue.length, 0);
  localStorage.removeItem('cratarr-queue-now-');
});

test('nobody signed in has no queue of anyone\'s, and hers comes back with her', async () => {
  await plays(track('a', 200));
  unreached();
  await runFor(11);
  accountForgotten();
  await settle();
  assert.equal(local.queue.length, 0);
  unreached();
  await settle();
  assert.equal(local.queue.length, 0, 'out of reach, nobody takes up nothing');
  assert.equal(perAccount('cratarr-queue-now'), 'cratarr-queue-now-');
  assert.equal(localStorage.getItem('cratarr-queue-now-'), null);
  accountLearnt('ann');
  await settle();
  assert.deepEqual(ids(local.queue), ['a']);
});

test('its name in the list of queues stays one: taken up, it is the queue that plays, not one left behind', async () => {
  await plays(track('a', 200));
  const id = keptHere.id;
  await runFor(1);
  nothingHeld();
  unreached();
  await settle();
  assert.equal(keptHere.id, id);
  assert.ok(!keptHere.list.some((k) => k.id === id));
});

test('a store it cannot read is no queue', async () => {
  localStorage.setItem('cratarr-queue-now@ann', '{not json');
  unreached();
  await settle();
  assert.equal(local.queue.length, 0);
  localStorage.setItem('cratarr-queue-now@ann', JSON.stringify({ tracks: [] }));
  unreached();
  await settle();
  assert.equal(local.queue.length, 0);
});
