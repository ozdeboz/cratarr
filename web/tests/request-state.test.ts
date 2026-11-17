// A request as every view reads it (W5, W12; lib/requestState): its state as the server works it out, in words, its fill, its
// cover's mark, Hand over again where the server says; a record not had's page says where its request stands.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { cardStanding, fillOf, handOverAgain, isOpen, markOfState, requestOpen, standing, standingTone } = await import('../src/lib/requestState.ts');
const { notHadPage } = await import('../src/lib/recordNotHad.ts');

const said = (t: number) => `at ${t}`;
const r = (state: Record<string, unknown> | null, extra: Record<string, unknown> = {}) => ({ status: 'approved', at: 5, ...(state ? { state: state as never } : {}), ...extra });

test("each state in its words, from the server's state", () => {
  assert.equal(standing(r({ state: 'waiting_for_approval' }, { status: 'pending' }), said), 'Waiting · at 5');
  assert.equal(standing(r({ state: 'declined' }, { reason: 'Not on vinyl' }), said), 'Declined: Not on vinyl');
  assert.equal(standing(r({ state: 'cancelled' }), said), 'Taken back · at 5');
  assert.equal(standing(r({ state: 'rejected' }), said), 'Rejected');
  assert.equal(standing(r({ state: 'done' }), said), 'In the library');
  assert.equal(standing(r({ state: 'arrived' }), said), 'Arrived');
  assert.equal(standing(r({ state: 'partly_there', has: 8, of: 10 }), said), 'Partly there · 8 of 10');
  assert.equal(standing(r({ state: 'wanted', lidarr: { state: 'looking' } }), said), 'Lidarr is looking for it');
  assert.equal(standing(r({ state: 'wanted', soulseek: { state: 'searching' } }), said), 'Soulseek is looking for it');
  assert.equal(standing(r({ state: 'wanted', soulseek: { state: 'waiting', why: "Bandcamp didn't answer" } }), said), "Bandcamp didn't answer");
  assert.equal(standing(r({ state: 'wanted' }, { fetched: 'Handed to Lidarr' }), said), 'Handed to Lidarr');
  assert.equal(standing(r({ state: 'wanted' }), said), 'Approved');
});

test('a fetch fills from Lidarr or Soulseek, the further; a card never says the fetcher\'s own words', () => {
  const fetching = r({ state: 'wanted', lidarr: { state: 'fetching', done: 0.2 }, soulseek: { state: 'fetching', percent: 40 } });
  assert.equal(fillOf(fetching.state), 0.4);
  assert.equal(standing(fetching, said), 'Being fetched · 40%');
  assert.equal(fillOf(r({ state: 'arrived' }).state), null);
  assert.equal(cardStanding(r({ state: 'wanted' }, { fetched: 'Handed to Lidarr' }), said), 'Approved');
  assert.equal(cardStanding(r({ state: 'waiting_for_approval' }, { status: 'pending' }), said), 'Requested');
  assert.equal(cardStanding(r({ state: 'wanted' }, { fetched: 'Not handed over: Lidarr is down' }), said), '');
});

test('open while waiting, wanted, partly there or arrived; a request just made by its status', () => {
  for (const s of ['waiting_for_approval', 'wanted', 'partly_there', 'arrived']) assert.ok(isOpen({ state: s } as never), s);
  for (const s of ['done', 'declined', 'cancelled', 'rejected']) assert.ok(!isOpen({ state: s } as never), s);
  assert.ok(requestOpen(r(null, { status: 'pending' })));
  assert.ok(!requestOpen(r(null, { status: 'declined' })));
  assert.ok(!requestOpen(r({ state: 'done' }, { status: 'approved' })), 'the state, not the status');
});

test("the cover's mark: coming once fetched or arrived, wanted while open, none once in the library or closed", () => {
  assert.deepEqual(markOfState(r({ state: 'arrived' })), { state: 'downloading', done: 1 });
  assert.deepEqual(markOfState(r({ state: 'wanted', soulseek: { state: 'fetching', percent: 50 } })), { state: 'downloading', done: 0.5 });
  assert.deepEqual(markOfState(r({ state: 'wanted' })), { state: 'wanted' });
  assert.equal(markOfState(r({ state: 'done' })), null);
  assert.equal(markOfState(r({ state: 'rejected' })), null);
});

test('Hand over again only where the server says no fetcher spoke, for one who answers requests; the bad ones said so', () => {
  assert.ok(handOverAgain(r({ state: 'wanted' }, { hand_over_again: true }), true));
  assert.ok(!handOverAgain(r({ state: 'wanted' }, { hand_over_again: true }), false));
  assert.ok(!handOverAgain(r({ state: 'wanted' }, { hand_over_again: false, fetched: 'Not handed over' }), true), 'the flag, not the words');
  assert.equal(standingTone(r({ state: 'rejected' })), 'bad');
  assert.equal(standingTone(r({ state: 'wanted', soulseek: { state: 'stuck' } })), 'bad');
  assert.equal(standingTone(r({ state: 'wanted' })), null);
});

test("a record not had's page says where its open request stands", () => {
  assert.deepEqual(notHadPage({ request: r({ state: 'partly_there', has: 3, of: 9 }) }, said).lines, [{ kind: 'request', text: 'Partly there · 3 of 9' }]);
  assert.deepEqual(notHadPage({ request: r({ state: 'declined' }) }, said).lines, []);
});
