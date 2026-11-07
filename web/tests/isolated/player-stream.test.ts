// The page's own player over a stream that breaks, a queue that changes and Repeat (lib/localPlayer, the real one, on
// tests/harness/fakeStream). What a test asks is what is heard and what the player shows of it.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { audio, fromRest, player, runFor, server, settle, track } from '../harness/fakeStream.ts';

const { clearUpcoming, dropFromQueue, jump, local, next, playNow, playQueue, previous, remove, removePlaying, setAutoplay, setRepeat, togglePlay } = player;

/** Whose sound is at the playing position. */
const heard = () => audio.heard()?.track ?? null;
/** The track the player shows. */
const shown = () => local.queue[local.index]?.id ?? null;
const near = (a: number, b: number, what: string) => assert.ok(Math.abs(a - b) < 1, `${what}: ${a}, not about ${b}`);

/** `tracks` from the first, its first moments in. */
async function plays(...tracks: ReturnType<typeof track>[]) {
  playQueue(tracks);
  await settle();
}

beforeEach(fromRest);

// --- A stream that breaks (the audit's 24) ---

test('a track that breaks while it plays goes on from what the stream holds, asked for once more from there', async () => {
  server.plans.set('a', [{ gives: 50, then: 'breaks' }]);
  await plays(track('a', 200), track('b', 200));
  await runFor(40);
  assert.equal(heard(), 'a', 'nothing laid over it');
  assert.equal(shown(), 'a');
  near(local.position, 40, 'the clock');
  near(audio.heard()!.at, 40, 'its own place');
  assert.deepEqual(server.asked.slice(0, 2), [{ track: 'a', from: 0 }, { track: 'a', from: 50 }], 'its rest asked for from where the stream stops');
  await runFor(100);
  assert.equal(heard(), 'a', 'past where it broke');
  near(audio.heard()!.at, 140, 'with nothing of it lost');
  assert.equal(local.error, null, 'mended, so nothing to say');
  await runFor(70);
  assert.equal(heard(), 'b', 'and the next follows it');
  near(audio.heard()!.at, 10, 'from its start');
});

test('a track that breaks again ends at what the stream held, and the next plays from its start after it', async () => {
  server.plans.set('a', [{ gives: 50, then: 'breaks' }, { gives: 20, then: 'breaks' }]);
  await plays(track('a', 200), track('b', 200));
  assert.ok(local.error, 'said');
  await runFor(65);
  assert.equal(heard(), 'a', 'all that came of it plays');
  near(audio.heard()!.at, 65, 'on towards its last moment');
  await runFor(15);
  assert.equal(heard(), 'b');
  assert.equal(shown(), 'b');
  near(local.position, 10, 'the next from its start, at the moment the first gave out');
  assert.equal(server.asked.filter((a) => a.track === 'a').length, 2, 'asked once more, no more than that');
});

test('a track not reached yet that breaks twice takes no time in the stream: the one after it plays in its place', async () => {
  server.plans.set('b', [{ gives: 5, then: 'breaks' }, { gives: 5, then: 'breaks' }]);
  await plays(track('a', 40), track('b', 100), track('c', 100));
  await runFor(30);
  assert.equal(heard(), 'a', 'what plays goes on');
  await runFor(20);
  assert.equal(heard(), 'c');
  assert.equal(shown(), 'c');
  near(local.position, 10, 'from its start');
  assert.equal(server.asked.filter((a) => a.track === 'b').length, 2, 'asked once more, no more than that');
});

test('a track not reached yet that breaks once loses nothing: asked for again, from its start', async () => {
  server.plans.set('b', [{ gives: 5, then: 'breaks' }]);
  await plays(track('a', 40), track('b', 100));
  await runFor(50);
  assert.equal(heard(), 'b');
  near(audio.heard()!.at, 10, 'whole, nothing of what came first laid over it');
  assert.equal(local.error, null, 'mended, so nothing to say');
});

test('a track that breaks with nothing of it held moves on at once, as one that never came', async () => {
  server.plans.set('a', [{ gives: 0, then: 'breaks' }, { gives: 0, then: 'breaks' }]);
  await plays(track('a', 100), track('b', 100));
  await runFor(5);
  assert.equal(shown(), 'b');
  assert.equal(heard(), 'b');
  near(local.position, 5, 'from its start');
});

test("the queue's last track that breaks plays out what the stream holds and ends there", async () => {
  server.plans.set('a', [{ gives: 50, then: 'breaks' }, { gives: 0, then: 'breaks' }]);
  await plays(track('a', 200));
  await runFor(40);
  assert.equal(audio.paused, false, 'not stopped with a minute of it in');
  assert.equal(heard(), 'a');
  await runFor(15);
  assert.equal(audio.ended, true);
  assert.equal(local.paused, true);
  assert.ok(local.error, 'said');
});

// --- The queue changed while the next track comes in (the audit's 25) ---

/** `a` in its last 30 seconds, so `b` has begun coming in, and stands there half way. */
async function nextComingIn() {
  server.plans.set('b', [{ gives: 10, then: 'hangs' }]);
  await plays(track('a', 60), track('b', 100));
  await runFor(35);
  assert.deepEqual(server.asked.map((a) => a.track), ['a', 'b'], 'b has been asked for');
  assert.equal(heard(), 'a');
}

test('Play now while the next track comes in plays the track asked for, and on past its first moments', async () => {
  await nextComingIn();
  playNow([track('x', 100)]);
  await settle();
  await runFor(2);
  assert.equal(shown(), 'x');
  assert.equal(heard(), 'x', 'its sound, not the one that was coming in');
  await runFor(50);
  assert.equal(heard(), 'x', 'no stall when what had come in would have run out');
  near(audio.heard()!.at, 52, 'and its place follows the clock');
});

test('Clear while the next track comes in leaves none of its sound in the stream', async () => {
  await nextComingIn();
  clearUpcoming();
  await settle();
  await runFor(30);
  assert.notEqual(heard(), 'b', 'what was taken out plays no more');
  assert.equal(shown(), 'a', 'and the place stays on the queue');
});

test('taking out the only track still to come while it comes in does the same', async () => {
  await nextComingIn();
  remove(1);
  await settle();
  await runFor(30);
  assert.notEqual(heard(), 'b');
  assert.equal(shown(), 'a');
});

test('a queue change behind what comes in keeps what is in and lets the new next one follow', async () => {
  server.plans.set('c', [{ gives: 10, then: 'hangs' }]);
  await plays(track('a', 40), track('b', 40), track('c', 100), track('d', 100));
  await runFor(45);
  assert.equal(shown(), 'b');
  remove(2);
  await settle();
  assert.deepEqual(local.queue.map((t) => t.id), ['a', 'b', 'd']);
  await runFor(45);
  assert.equal(heard(), 'd', 'b ran on into d, nothing of c');
  near(audio.heard()!.at, 10, 'from its start');
});

// --- Excluding the playing track at the queue's end (the audit's 72) ---

test('taking the queue\'s last track out while it plays stands on the track before it, ready to play', async () => {
  await plays(track('a', 100), track('b', 100));
  jump(1);
  await settle();
  await runFor(5);
  assert.equal(heard(), 'b');
  dropFromQueue((t) => t.id === 'b');
  await settle();
  await runFor(1);
  assert.deepEqual(local.queue.map((t) => t.id), ['a']);
  assert.equal(local.index, 0, 'the clock does not put it back past the end');
  assert.equal(shown(), 'a', 'so the player stays');
  assert.equal(local.paused, true, 'the queue ended there, paused');
  togglePlay();
  await settle();
  await runFor(3);
  assert.equal(heard(), 'a', 'and it can be taken up');
  near(audio.heard()!.at, 3, 'from its start');
});

test('the only track taken out while it plays leaves nothing to show', async () => {
  await plays(track('a', 100));
  await runFor(5);
  removePlaying();
  await settle();
  await runFor(1);
  assert.equal(local.index, -1);
  assert.equal(local.queue.length, 0);
  assert.equal(audio.paused, true);
});

// --- Nothing left to come (the audit's 73) ---

/** Runs out what plays: the element at its end, and the player showing so. */
function endsThere(what: string) {
  assert.equal(audio.ended, true, `${what}: the element ended, not waiting`);
  assert.equal(local.paused, true, what);
  assert.equal(local.starting, false, `${what}: not shown as about to play`);
}

test('Clear with the playing track wholly in ends the stream, so the track ends and the player says so', async () => {
  await plays(track('a', 50), track('b', 100));
  await runFor(5);
  assert.deepEqual(server.asked.map((t) => t.track), ['a'], 'b not begun');
  clearUpcoming();
  await settle();
  await runFor(47);
  endsThere('cleared');
  assert.equal(shown(), 'a');
});

test('taking out the last track with the one before it in ends the stream after that one', async () => {
  await plays(track('a', 40), track('b', 40), track('c', 100));
  await runFor(30);
  assert.deepEqual(server.asked.map((t) => t.track), ['a', 'b'], 'c not begun');
  remove(2);
  await settle();
  await runFor(52);
  endsThere('b the last now');
  assert.equal(shown(), 'b');
});

test('taking out a track from a stream already ended ends it again at the track before', async () => {
  await plays(track('a', 30), track('b', 30));
  await runFor(5);
  assert.deepEqual(server.asked.map((t) => t.track), ['a', 'b'], 'all of it in');
  remove(1);
  await settle();
  await runFor(27);
  endsThere('a alone');
  assert.equal(shown(), 'a');
});

test('Clear with the next track coming in ends the stream too', async () => {
  await nextComingIn();
  clearUpcoming();
  await settle();
  await runFor(27);
  endsThere('cleared while b came in');
});

test('a track added after the stream ended still plays on from it', async () => {
  await plays(track('a', 50), track('b', 100));
  await runFor(5);
  clearUpcoming();
  await settle();
  player.enqueue([track('c', 100)], 'last');
  await settle();
  await runFor(50);
  assert.equal(heard(), 'c');
  assert.equal(shown(), 'c');
  near(audio.heard()!.at, 5, 'run into from its start');
});

test('Autoplay fills again after a Clear, and the music runs on into its picks', async () => {
  await plays(track('a', 50), track('b', 100));
  setAutoplay('similar');
  await runFor(5);
  server.picks = [track('p', 100)];
  clearUpcoming();
  await settle();
  assert.deepEqual(local.queue.map((t) => t.id), ['a', 'p'], 'picked at once: no next track is coming to ask at');
  await runFor(50);
  assert.equal(heard(), 'p');
  assert.equal(shown(), 'p');
});

test('Autoplay fills again after the only track to come is taken out', async () => {
  await plays(track('a', 50), track('b', 100));
  setAutoplay('similar');
  await runFor(5);
  server.picks = [track('p', 100)];
  remove(1);
  await settle();
  assert.deepEqual(local.queue.map((t) => t.id), ['a', 'p']);
});

// --- Repeat (the audit's 29) ---

test('Repeat all goes round from a last track reached by Next', async () => {
  await plays(track('a', 60), track('b', 40));
  setRepeat('all');
  next();
  await settle();
  assert.equal(shown(), 'b');
  assert.ok(local.index < local.queue.length - 1, 'the next round is there');
  await runFor(45);
  assert.equal(heard(), 'a', 'the stream goes on round');
  assert.equal(shown(), 'a');
  near(audio.heard()!.at, 5, 'from its start');
});

test('Repeat all goes round from a last track reached by a tap', async () => {
  await plays(track('a', 60), track('b', 40), track('c', 40));
  setRepeat('all');
  jump(2);
  await settle();
  await runFor(45);
  assert.equal(heard(), 'a');
  near(audio.heard()!.at, 5, 'from its start');
});

test('Repeat all set before a single track starts plays it round, and Next on it moves on', async () => {
  setRepeat('all');
  await plays(track('a', 40));
  assert.equal(local.queue.length, 2, 'its next round is there from its start');
  await runFor(45);
  assert.equal(heard(), 'a', 'round again');
  near(audio.heard()!.at, 5, 'from its start');
  const was = local.index;
  next();
  await settle();
  assert.equal(local.index, was + 1, 'the last track never stands without a next');
  assert.equal(heard(), 'a');
});

test('Repeat all goes round after what followed the playing track is cleared', async () => {
  await plays(track('a', 50), track('b', 40));
  setRepeat('all');
  await runFor(5);
  clearUpcoming();
  await settle();
  await runFor(50);
  assert.equal(heard(), 'a', 'lit, so it goes round');
  near(audio.heard()!.at, 5, 'from its start');
});

test('Repeat all goes round from a last track that takes the place of the one taken out', async () => {
  await plays(track('a', 60), track('b', 60), track('c', 40));
  setRepeat('all');
  jump(1);
  await settle();
  removePlaying();
  await settle();
  assert.equal(shown(), 'c');
  await runFor(45);
  assert.equal(heard(), 'a');
  near(audio.heard()!.at, 5, 'from its start');
});

test('Previous on the first track under Repeat all goes to the last, and round from there', async () => {
  await plays(track('a', 60), track('b', 40));
  setRepeat('all');
  previous();
  await settle();
  assert.equal(shown(), 'b');
  await runFor(45);
  assert.equal(heard(), 'a');
});

test('Repeat one plays the queue\'s only track again from its start', async () => {
  await plays(track('a', 20));
  setRepeat('one');
  await runFor(25);
  assert.equal(heard(), 'a', 'round again');
  assert.equal(audio.paused, false);
  assert.equal(local.paused, false);
  near(audio.heard()!.at, 5, 'from its start');
  assert.equal(local.index, 0);
  assert.equal(local.queue.length, 1, 'nothing added');
});

test('Repeat one plays the queue\'s last track again, one too long to be held whole', async () => {
  await plays(track('a', 60), track('b', 100));
  jump(1);
  await settle();
  setRepeat('one');
  await runFor(105);
  assert.equal(shown(), 'b');
  assert.equal(heard(), 'b');
  near(audio.heard()!.at, 5, 'from its start');
  assert.equal(audio.paused, false);
});

test('Repeat one turns the track round where another follows it, its start gone from the stream or not', async () => {
  await plays(track('a', 45), track('b', 100));
  setRepeat('one');
  await runFor(50);
  assert.equal(shown(), 'a');
  assert.equal(heard(), 'a', 'the next one heard for no more than a moment');
  assert.ok(audio.heard()!.at < 6, `back at its start, at ${audio.heard()!.at}`);
});

test("with no repeat the queue's last track ends the music", async () => {
  await plays(track('a', 20));
  await runFor(25);
  endsThere('over');
  assert.equal(local.queue.length, 1);
});

test("the queue's last track ends the music however long it is: dropping what was played does not open the stream again", async () => {
  await plays(track('a', 200));
  await runFor(205);
  endsThere('over');
});
