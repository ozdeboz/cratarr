// Holding on server loss, the owner's call of 2026-11-06 (lib/localPlayer, the real one, on tests/harness/fakeStream): with
// Cratarr gone the player stops at the place reached, keeps the queue, and carries on by itself when Cratarr answers. What a
// test asks is what is heard and what the player shows of it.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { audio, fromRest, player, runFor, server, settle, track } from '../harness/fakeStream.ts';

const { enqueue, jump, local, next, playQueue, remove, seek, setAutoplay, togglePlay } = player;
const { reach, reached } = await import('../../src/lib/reach.svelte.ts');
const { readsNow } = await import('../../src/lib/deviceIndex.svelte.ts');
const { heldAt, holdAfterCut, HOLD_FIRST_MS, HOLD_MOST_MS } = await import('../../src/lib/player/holdOn.ts');

const heard = () => audio.heard()?.track ?? null;
const shown = () => local.queue[local.index]?.id ?? null;
const near = (a: number, b: number, what: string) => assert.ok(Math.abs(a - b) < 1, `${what}: ${a}, not about ${b}`);
const asked = (id: string) => server.asked.filter((a) => a.track === id);

async function plays(...tracks: ReturnType<typeof track>[]) {
  playQueue(tracks);
  await settle();
}

/** Cratarr answers, as any request of the page's finds. */
async function answers() {
  server.away = false;
  server.answer = null;
  reached();
  await settle();
}

/** `a` plays out with Cratarr gone before `b` was asked for: the player stands at its end. */
async function heldAtTheEndOfA() {
  await plays(track('a', 60), track('b', 100), track('c', 100));
  await runFor(5);
  server.away = true;
  await runFor(65);
}

beforeEach(async () => {
  reach.away = false;
  await fromRest();
});

test('Cratarr gone before the next track came: what the stream holds plays out, and the player stands there, the queue whole', async () => {
  await heldAtTheEndOfA();
  assert.equal(heard(), null, 'it has run out');
  assert.equal(shown(), 'a', 'on the track it reached');
  near(local.position, 60, 'at its end');
  assert.deepEqual(local.queue.map((t) => t.id), ['a', 'b', 'c']);
  assert.equal(asked('c').length, 0, 'none past the one it waits for: the queue does not run out behind the sound');
  assert.equal(audio.ended, false, 'its stream stays open for what comes');
});

test('the hold goes by no settling: the player has stopped while reads are still on the server, for the look that is due', async () => {
  await heldAtTheEndOfA();
  assert.equal(local.starting, true, 'held');
  assert.equal(reach.away, true);
  assert.equal(readsNow().from, 'server', 'A1 is the reads\' and the pages\', and stopping is not moving a page');
});

test('it shows the truth of that: asked to play, not sounding, and no failure to report', async () => {
  await heldAtTheEndOfA();
  assert.equal(local.paused, false);
  assert.equal(local.starting, true);
  assert.equal(local.error, null);
  assert.equal(reach.away, true, 'and the page knows Cratarr is out of reach');
});

test('Cratarr answering: it carries on by itself, the next track from its start', async () => {
  await heldAtTheEndOfA();
  await answers();
  await runFor(3);
  assert.equal(shown(), 'b');
  assert.equal(heard(), 'b');
  near(audio.heard()!.at, 3, 'from its start, nothing passed over');
  assert.equal(local.starting, false);
  assert.equal(audio.paused, false);
});

test('come back, nothing is held any more: the track is not asked for twice, and what follows it comes in its turn', async () => {
  await heldAtTheEndOfA();
  await answers();
  const asks = asked('b').length;
  await runFor(95);
  assert.equal(asked('b').length, asks);
  assert.equal(shown(), 'b');
  assert.equal(asked('c').length, 1);
  await runFor(10);
  assert.equal(heard(), 'c');
});

test('with nobody to say Cratarr answers, its own asks find out', async () => {
  await heldAtTheEndOfA();
  server.away = false;
  await runFor(31);
  assert.equal(heard(), 'b', 'within the longest wait between two asks');
});

test('its asks grow further apart, from a moment to half a minute', async () => {
  await plays(track('a', 40), track('b', 100));
  server.away = true;
  await runFor(10);
  await runFor(140);
  const n = asked('b').length;
  assert.ok(n >= 6 && n <= 10, `${n} asks in about two minutes`);
  assert.equal(HOLD_FIRST_MS, 2_000);
  assert.equal(HOLD_MOST_MS, 30_000);
  const first = heldAt(null, { index: 1, from: 0, whole: false }, 0);
  assert.equal(first.askAt, 2_000);
  const second = heldAt(first, { index: 1, from: 0, whole: false }, 2_000);
  assert.equal(second.askAt, 6_000);
  let h = second;
  for (let i = 0; i < 12; i++) h = heldAt(h, { index: 1, from: 0, whole: false }, 0);
  assert.equal(h.askAt, HOLD_MOST_MS);
  assert.equal(heldAt(h, { index: 2, from: 0, whole: false }, 0).askAt, 2_000, 'another track waits from the start');
});

test('a stream cut off mid-track holds where it stopped, and goes on from there with nothing lost', async () => {
  server.plans.set('a', [{ gives: 100, then: 'dies' }]);
  await plays(track('a', 300), track('b', 100));
  await runFor(110);
  assert.equal(heard(), null);
  assert.equal(shown(), 'a');
  near(local.position, 100, 'at the place reached');
  assert.equal(local.error, null);
  await answers();
  await runFor(5);
  assert.equal(heard(), 'a', 'the same track');
  near(audio.heard()!.at, 105, 'from where it stopped');
  assert.equal(asked('a').at(-1)!.from, 100);
  assert.equal(asked('b').length, 0);
});

test('a stream cut off on the queue\'s last track holds too: the queue has not ended', async () => {
  server.plans.set('a', [{ gives: 100, then: 'dies' }]);
  await plays(track('a', 300));
  await runFor(110);
  assert.equal(audio.ended, false);
  assert.equal(local.paused, false);
  await answers();
  await runFor(5);
  near(audio.heard()!.at, 105, 'and goes on');
});

test('Cratarr going as the next track comes in: none of it stays, and it is held at its start', async () => {
  server.plans.set('b', [{ gives: 15, then: 'dies' }]);
  await plays(track('a', 40), track('b', 100), track('c', 100));
  await runFor(20);
  assert.equal(heard(), 'a', 'what plays goes on');
  await runFor(25);
  assert.equal(heard(), null, 'nothing of b past the end of a');
  assert.equal(shown(), 'a');
  assert.equal(asked('c').length, 0);
  await answers();
  await runFor(3);
  assert.equal(heard(), 'b');
  near(audio.heard()!.at, 3, 'whole');
});

test('a place out of what the stream holds, with Cratarr gone, is held for, and heard once it answers', async () => {
  await plays(track('a', 300), track('b', 100));
  await runFor(5);
  server.away = true;
  seek(200, false);
  await settle();
  await runFor(5);
  assert.equal(shown(), 'a');
  near(local.position, 200, 'where it was asked to go');
  assert.equal(heard(), null);
  await answers();
  await runFor(2);
  assert.equal(heard(), 'a');
  near(audio.heard()!.at, 202, 'from there');
});

test('a queue played with Cratarr gone stands at its start, whole', async () => {
  server.away = true;
  await plays(track('a', 100), track('b', 100));
  await runFor(10);
  assert.deepEqual(local.queue.map((t) => t.id), ['a', 'b']);
  assert.equal(shown(), 'a');
  assert.equal(asked('b').length, 0);
  assert.equal(local.error, null);
  await answers();
  await runFor(2);
  assert.equal(heard(), 'a');
  near(audio.heard()!.at, 2, 'from its start');
});

test('a proxy\'s empty 502 is Cratarr gone, not a track that will not play', async () => {
  await plays(track('a', 40), track('b', 100), track('c', 100));
  server.answer = { status: 502 };
  await runFor(50);
  assert.equal(shown(), 'a');
  assert.equal(asked('c').length, 0);
  assert.equal(local.error, null);
  assert.equal(reach.away, true);
  await answers();
  await runFor(2);
  assert.equal(heard(), 'b');
});

test('Cratarr\'s own refusal of a track moves on as ever: it answered', async () => {
  server.refused.set('b', 'That file is gone.');
  await plays(track('a', 40), track('b', 100), track('c', 100));
  await runFor(35);
  assert.ok(local.error?.includes('That file is gone.'), 'said');
  await runFor(15);
  assert.equal(heard(), 'c');
  assert.equal(asked('b').length, 1, 'no waiting for it');
});

test('what is kept on the device plays in its turn, and the hold comes at the first track that is not', async () => {
  server.kept.add('b');
  await plays(track('a', 40), track('b', 40), track('c', 100));
  server.away = true;
  await runFor(45);
  assert.equal(heard(), 'b');
  await runFor(40);
  assert.equal(heard(), null);
  assert.equal(shown(), 'b');
  near(local.position, 40, 'at its end');
  await answers();
  await runFor(2);
  assert.equal(heard(), 'c');
});

test('a stall with nothing in while it holds says nothing of the form the track came in', async () => {
  server.opus = true;
  server.away = true;
  await plays(track('a', 100));
  await runFor(3);
  const before = server.addresses.length;
  audio.dispatchEvent(new Event('stalled'));
  await settle();
  assert.equal(server.addresses.length, before, 'no fresh stream for it');
  await answers();
  await runFor(2);
  assert.ok(server.addresses.every((a) => !a.includes('as=')), 'and it comes as it would have');
});

test('paused while it holds, it stays paused when Cratarr answers, and plays on from there at a press', async () => {
  await heldAtTheEndOfA();
  togglePlay();
  await settle();
  assert.equal(audio.paused, true);
  await answers();
  await runFor(5);
  assert.equal(audio.paused, true, 'no sound of its own accord');
  assert.equal(local.paused, true);
  assert.equal(asked('b').length > 0, true);
  togglePlay();
  await settle();
  await runFor(2);
  assert.equal(heard(), 'b');
});

test('a tap on another track while it holds plays that one, and what it waited for is let go', async () => {
  server.kept.add('c');
  await heldAtTheEndOfA();
  const before = asked('b').length;
  jump(2);
  await settle();
  await runFor(40);
  assert.equal(heard(), 'c');
  assert.equal(asked('b').length, before, 'b is not asked for any more');
});

test('Next while it holds goes on to the next track, and waits there', async () => {
  await heldAtTheEndOfA();
  next();
  await settle();
  await runFor(2);
  assert.equal(shown(), 'b');
  assert.equal(heard(), null);
  await answers();
  await runFor(2);
  assert.equal(heard(), 'b');
});

test('the track held for taken out of the queue: what follows it is asked for in its place', async () => {
  await heldAtTheEndOfA();
  remove(1);
  await settle();
  await answers();
  await runFor(2);
  assert.equal(heard(), 'c');
  near(audio.heard()!.at, 2, 'from its start');
});

test('a track put in before the one held for comes first: the player holds for it instead', async () => {
  server.kept.add('b');
  await plays(track('a', 100), track('b', 15), track('c', 100));
  server.away = true;
  await runFor(90);
  assert.equal(asked('b').length, 1, 'b is in');
  assert.ok(asked('c').length > 0, 'and c held for');
  enqueue([track('x', 100)], 'next');
  await settle();
  await runFor(15);
  assert.equal(heard(), null, 'a played out, and x is not here');
  assert.equal(shown(), 'a');
  await answers();
  await runFor(2);
  assert.equal(heard(), 'x');
});

test('the only track held for taken out: nothing is waited for, and the queue ends where it stands', async () => {
  await plays(track('a', 60), track('b', 100));
  await runFor(5);
  server.away = true;
  await runFor(65);
  remove(1);
  await settle();
  await runFor(2);
  assert.equal(audio.ended, true);
  assert.equal(local.paused, true);
  server.asked.length = 0;
  await answers();
  await runFor(35);
  assert.deepEqual(server.asked, []);
});

test('a change further on leaves the hold as it is', async () => {
  const h = heldAt(null, { index: 2, from: 30, whole: false }, 0);
  assert.equal(holdAfterCut(h, 3, 9), h);
  assert.equal(holdAfterCut(null, 0, 9), null);
  assert.deepEqual({ ...holdAfterCut(h, 1, 9) }, { ...h, index: 1, from: 0, whole: false, tries: 0 });
  assert.equal(holdAfterCut(h, 2, 2), null);
});

test('Autoplay out of reach says nothing, and asks again as Cratarr answers', async () => {
  await plays(track('a', 200));
  server.away = true;
  setAutoplay('similar');
  await settle();
  assert.equal(local.error, null, 'no failure to report');
  server.picks = [track('p', 100)];
  await answers();
  assert.deepEqual(local.queue.map((t) => t.id), ['a', 'p']);
  setAutoplay('stop');
});

test('a proxy\'s bare 403 holds too, and waits out the refusal: no knocking on a ban', async () => {
  await plays(track('a', 40), track('b', 100));
  server.answer = { status: 403 };
  await runFor(120);
  assert.equal(asked('b').length, 1, 'once, then nothing while it refuses');
  assert.equal(shown(), 'a');
  server.answer = null;
  await runFor(200);
  assert.equal(heard(), 'b', 'and on when it has passed');
});
