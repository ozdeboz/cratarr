// A record's Edit (lib/recordEdit; 0.13, per-record overrides): what one record or several show (shared or mixed, set
// by hand or found), and what Save sends: only what changed, null to go back to what Cratarr finds.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { classicalChoiceOf, classicalOfChoice, editableOf, isSecondary, sharedOf, shownOf, toSend, wordsOfView, yearOf } = await import('../src/lib/recordEdit.ts');

const rec = (o: Partial<ReturnType<typeof editableOf>> = {}) => ({ type: 'album', secondary: [], year: 1994, genres: ['Jazz'], classical: false, styles: [], moods: [], tags: [], set: [], ...o });

test('several records share a value or are mixed; set by hand when any has it set', () => {
  const two = [rec(), rec({ type: 'ep', set: ['type'] })];
  assert.deepEqual(sharedOf(two, 'type'), { value: null, mixed: true, set: true });
  assert.deepEqual(sharedOf(two, 'year'), { value: 1994, mixed: false, set: false });
  // Lists compare as sets.
  assert.deepEqual(sharedOf([rec({ genres: ['A', 'B'] }), rec({ genres: ['B', 'A'] })], 'genres').mixed, false);
});

test('Save sends only what changed: a value to set, null to reset what is set, nothing for a no-op', () => {
  const one = [rec({ type: 'ep', set: ['type'] })];
  assert.deepEqual(toSend(one, {}), {});
  // Setting what it already has set by hand changes nothing.
  assert.deepEqual(toSend(one, { type: 'ep' }), {});
  assert.deepEqual(toSend(one, { type: 'single' }), { type: 'single' });
  // A reset of a field set by hand goes; of one none has set, it does not.
  assert.deepEqual(toSend(one, { type: null, year: null }), { type: null });
  // Pinning the value Cratarr found is a change (it was not set by hand).
  assert.deepEqual(toSend([rec()], { year: 1994 }), { year: 1994 });
  // A list is sent once each.
  assert.deepEqual(toSend([rec()], { genres: ['Jazz', 'Jazz', 'Soul'] }), { genres: ['Jazz', 'Soul'] });
});

test('several: a mixed field left alone unless changed; changed, it goes to all', () => {
  const two = [rec({ year: 1990 }), rec({ year: 1991 })];
  assert.deepEqual(toSend(two, {}), {});
  assert.deepEqual(toSend(two, { year: 1995 }), { year: 1995 });
  // One of them set by hand already to it, the other not: still sent.
  assert.deepEqual(toSend([rec({ year: 1995, set: ['year'] }), rec({ year: 1995 })], { year: 1995 }), { year: 1995 });
});

test('what a row shows: the draft where changed (a reset as found), else what they share', () => {
  const one = [rec({ type: 'ep', set: ['type'] })];
  assert.equal(shownOf(one, { type: null }, 'type').reset, true);
  assert.deepEqual(shownOf(one, { type: 'single' }, 'type'), { value: 'single', mixed: false, set: true, reset: false });
  assert.deepEqual(shownOf(one, {}, 'type'), { value: 'ep', mixed: false, set: true, reset: false });
});

test('classical: auto unless marked; marked yes or no; mixed where some differ', () => {
  assert.equal(classicalChoiceOf([rec({ classical: true })], {}), 'auto');
  assert.equal(classicalChoiceOf([rec({ classical: true, set: ['classical'] })], {}), 'yes');
  assert.equal(classicalChoiceOf([rec({ classical: false, set: ['classical'] })], {}), 'no');
  assert.equal(classicalChoiceOf([rec({ classical: true, set: ['classical'] }), rec()], {}), '');
  assert.equal(classicalChoiceOf([rec()], { classical: true }), 'yes');
  assert.equal(classicalChoiceOf([rec({ set: ['classical'] })], { classical: null }), 'auto');
  assert.equal(classicalOfChoice('auto'), null);
  assert.equal(classicalOfChoice('no'), false);
});

test("a listed record read: its set fields, a 0.10 classical mark as set; a year typed", () => {
  const r = editableOf({ type: 'single', secondary: ['live'], year: 2001, classical: true, classical_mark: 'classical', set: ['type'] });
  assert.deepEqual(r.set, ['type', 'classical']);
  assert.equal(r.genres, null);
  assert.equal(editableOf({ year: null }).type, null);
  assert.equal(yearOf(' 1977 '), 1977);
  assert.equal(yearOf('77'), null);
  assert.equal(yearOf('19.5'), null);
});

test("only MusicBrainz's twelve secondary types are offered and sent", () => {
  assert.equal(isSecondary('dj-mix'), true);
  assert.equal(isSecondary('audio drama'), true);
  assert.equal(isSecondary('Live'), false);
  assert.equal(isSecondary('bootleg'), false);
});

test("the words' hand layer (decision 0097): styles, moods and tags set and reset as genres are", () => {
  const one = [rec({ styles: ['Hard Bop'], set: ['styles'] })];
  assert.deepEqual(toSend(one, { styles: ['Hard Bop', 'Cool', 'Cool'] }), { styles: ['Hard Bop', 'Cool'] });
  assert.deepEqual(toSend(one, { styles: null, tags: null }), { styles: null });
  assert.deepEqual(toSend(one, { moods: ['calm'] }), { moods: ['calm'] });
  // One record's words from its view: each kind's words, genre from `genres` only where the view has none.
  const v = { genres: ['Jazz'], words: { kinds: [{ kind: 'style', words: [{ word: 'Hard Bop' }] }, { kind: 'tag', words: [{ word: 'late night' }] }] } };
  assert.deepEqual(wordsOfView(v), { genres: [], styles: ['Hard Bop'], moods: [], tags: ['late night'] });
  assert.deepEqual(wordsOfView({ genres: ['Jazz'] }).genres, ['Jazz']);
  const r = editableOf({ year: 2001 }, wordsOfView(v));
  assert.equal(r.tags?.[0], 'late night');
  assert.equal(editableOf({ year: 2001 }).moods, null);
});
