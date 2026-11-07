// The plays not yet told (lib/playsToTell): Cratarr leaves out of `done` a play it cannot count yet, and the page is not to
// go straight back with it.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { playsToTell, TELL_AGAIN_MS } = await import('../src/lib/playsToTell.ts');

type Play = { id: string; at: number; account?: string };
const play = (id: string, account?: string): Play => ({ id, at: 1_000_000, ...(account === undefined ? {} : { account }) });
/** What is on its way, answered. */
const settled = () => new Promise((ok) => setTimeout(ok, 0));

/** A telling whose reports `answer` answers, on a clock of its own, signed in as ann unless `who` says. Answers all after
 *  twenty, so going round fails a test instead of hanging it. */
function telling(answer: (ids: string[]) => string[] | Promise<string[]>, who: () => string | null = () => 'ann') {
  const t = { clock: 5_000_000, sent: [] as string[][], kept: [] as Play[] };
  const plays = playsToTell<Play>(
    async (told) => {
      const ids = told.map((p) => p.id);
      t.sent.push(ids);
      return { done: t.sent.length > 20 ? ids : await answer(ids) };
    },
    who,
    (list) => (t.kept = list),
    () => t.clock,
  );
  return { t, plays };
}

test('a play Cratarr cannot count yet is not sent straight back, only after a pause', async () => {
  const { t, plays } = telling(() => []);
  plays.add(play('a', 'ann'));
  await settled();
  assert.deepEqual(t.sent, [['a']], 'once, not round and round');
  assert.deepEqual(t.kept.map((p) => p.id), ['a'], 'kept for later');
  // Each answer of Cratarr's calls `tell` (lib/reach): a busy page must not undo the pause.
  plays.tell();
  await settled();
  t.clock += TELL_AGAIN_MS - 1;
  plays.tell();
  await settled();
  assert.equal(t.sent.length, 1);
  t.clock += 1;
  plays.tell();
  await settled();
  assert.deepEqual(t.sent, [['a'], ['a']]);
});

test('a report that took some goes straight on with the rest, and pauses only once nothing moves', async () => {
  const { t, plays } = telling((ids) => ids.filter((id) => id !== 'stuck'));
  plays.add(play('a', 'ann'));
  plays.add(play('stuck', 'ann'));
  plays.add(play('c', 'ann'));
  await settled();
  assert.deepEqual(t.sent, [['a'], ['stuck', 'c'], ['stuck']]);
  assert.deepEqual(t.kept.map((p) => p.id), ['stuck']);
});

test('a report that failed keeps its plays and pauses too', async () => {
  const { t, plays } = telling(() => Promise.reject(new Error('Server returned 500')));
  plays.add(play('a', 'ann'));
  await settled();
  plays.tell();
  await settled();
  assert.deepEqual(t.sent, [['a']]);
  assert.deepEqual(t.kept.map((p) => p.id), ['a']);
});

test('all taken, nothing waits and the next is told at once', async () => {
  const { t, plays } = telling((ids) => ids);
  plays.add(play('a', 'ann'));
  await settled();
  assert.deepEqual(t.kept, []);
  plays.add(play('b', 'ann'));
  await settled();
  assert.deepEqual(t.sent, [['a'], ['b']]);
});

test('whose: only the account signed in is told; another waits, and one heard with nobody is no one’s, for the next as for any', async () => {
  let who: string | null = null;
  const { t, plays } = telling((ids) => ids, () => who);
  plays.add(play('bob', 'bob'));
  plays.add(play('heard-as-she-signed-out'));
  await settled();
  assert.deepEqual(t.sent, [], 'not known yet: none told');
  who = 'ann';
  plays.tell();
  await settled();
  assert.deepEqual(t.sent, [], 'not hers, for coming next');
  assert.deepEqual(t.kept, [play('bob', 'bob')], 'kept for bob, and nothing else');
});

test('one kept with no account by a page before this one is not taken in', async () => {
  const { t, plays } = telling((ids) => ids);
  plays.took([play('no-ones'), play('hers', 'ann')]);
  assert.deepEqual(t.kept, [play('hers', 'ann')]);
  plays.tell();
  await settled();
  assert.deepEqual(t.sent, [['hers']]);
});

test('each account keeps its latest apart: his plays do not push out hers', () => {
  const { t, plays } = telling(() => [], () => null);
  plays.add(play('hers', 'ann'));
  for (let i = 0; i < 600; i++) plays.add(play(`his-${i}`, 'bo'));
  assert.ok(t.kept.some((p) => p.id === 'hers'));
  assert.equal(t.kept.length, 501);
  assert.equal(t.kept.at(-1)?.id, 'his-599');
  assert.deepEqual(t.kept.slice(0, 2).map((p) => p.id), ['hers', 'his-100'], 'in order, the oldest of his gone');
});

test('a telling can be waited for, to the end of what waits: a sign-out goes after it', async () => {
  const { t, plays } = telling(async (ids) => {
    await settled();
    return ids.slice(0, 1);
  });
  plays.took([play('a', 'ann'), play('b', 'ann')]);
  await plays.tell();
  assert.deepEqual(t.sent, [['a', 'b'], ['b']]);
  assert.deepEqual(t.kept, []);
});

test('what was kept before this page comes in once, by its id', async () => {
  const { t, plays } = telling((ids) => ids);
  plays.took([play('a', 'ann'), play('b', 'ann')]);
  plays.took([play('b', 'ann')]);
  plays.tell();
  await settled();
  assert.deepEqual(t.sent, [['a', 'b']]);
});
