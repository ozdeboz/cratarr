// Health › Activity (RN item 6, SCENARIOS-RN AC1, AC2, AC4): every job that runs or waits, a row each, its words from the jobs' one
// table (shared/states.json, the server's too), a time by the page's `when()`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { WORDS, activityItem, lineOf, patched } = await import('../src/lib/activityRows.ts');
const shared = JSON.parse(readFileSync(new URL('../../shared/states.json', import.meta.url), 'utf8'));
const when = () => '14:05';

const pass = { id: 'job:1', kind: 'analysis', state: 'running', what: 'Band waves', detail: 'Glass Harbour/Night Ferry/03 Neap.flac · catching up', done: 1204, of: 3580, started: 0 } as const;
const check = { id: 'job:2', kind: 'soulseek:check', state: 'running', what: 'Checking Glass Harbour – Tide Tables', done: 4, of: 9, unit: 'files', started: 0 } as const;
const imp = { id: 'job:3', kind: 'import:lastfm', state: 'running', what: 'Reading scrobbles', done: 120, of: 400, whose: 'alice', started: 0 } as const;
const typing = { id: 'job:4', kind: 'discogs:type', state: 'waiting', what: 'Typing records', done: 30, of: 300, waits: { what: '', until: 1 }, started: 0 } as const;
const held = { id: 'intake:ti-11', kind: 'intake:check', state: 'waiting', what: 'Checking', detail: 'Glass Harbour – Tide Tables · 3. Spring Tide', waits: { what: 'AcoustID asked Cratarr to wait', until: 1 }, started: 0 } as const;
const off = { id: 'job:5', kind: 'discogs:type', state: 'waiting', what: 'Typing records', waits: { what: 'Background reading is paused', until: null }, started: 0 } as const;

test("the jobs' words are the one table's (shared/states.json)", () => {
  assert.deepEqual(WORDS, shared.words);
});

test('activity rows say what each job does and how far', () => {
  assert.equal(lineOf(pass, when), 'Band waves · Glass Harbour/Night Ferry/03 Neap.flac · catching up · 1,204 of 3,580');
  assert.equal(lineOf(check, when), 'Checking Glass Harbour – Tide Tables · 4 of 9 files');
  assert.equal(lineOf(typing, when), 'Typing records · 30 of 300 · Paused until 14:05', 'a source paused says only until when');
  assert.equal(lineOf(held, when), 'Checking · Glass Harbour – Tide Tables · 3. Spring Tide · AcoustID asked Cratarr to wait · Paused until 14:05', 'a hold its own words, then its time');
  assert.equal(lineOf(off, when), 'Typing records · Background reading is paused', 'a switch has no deadline');
  const row = activityItem(imp, when);
  assert.deepEqual([row.name, row.value, row.line, row.progress], ['Reading scrobbles', '120 of 400', "alice's", 0.3], "whose, how far, its bar");
  assert.equal(activityItem(pass, when).line, 'Glass Harbour/Night Ferry/03 Neap.flac · catching up', "Cratarr's own: no one's name");
  assert.equal(activityItem(typing, when).waiting, true);
});

test('a patch replaces its job where it is, and one that ended leaves', () => {
  const jobs = [pass, check, imp];
  const moved = patched(jobs, { ...check, done: 5 });
  assert.deepEqual(moved.map((j) => [j.id, j.done]), [['job:1', 1204], ['job:2', 5], ['job:3', 120]]);
  assert.deepEqual(patched(moved, { ...imp, state: 'done', finished: 2 }).map((j) => j.id), ['job:1', 'job:2']);
  assert.deepEqual(patched([], typing).map((j) => j.id), ['job:4'], 'a new one joins');
});
