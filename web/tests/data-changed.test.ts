// What the page shows is read again when the server says it changed (lib/dataChanges; the owner, 2026-11-03: after a
// fix the level above showed what was before). The server's word, not each action's own reload.
import { test, mock } from 'node:test';
import assert from 'node:assert/strict';

const { changesOn } = await import('../src/lib/dataChanges.ts');
// The live channel's events, stood in for.
const handlers = new Map<string, Set<(d: unknown) => void>>();
const on = <T>(name: string, h: (d: T) => void) => {
  const set = handlers.get(name) ?? new Set();
  handlers.set(name, set);
  set.add(h as (d: unknown) => void);
  return () => void set.delete(h as (d: unknown) => void);
};
const emit = (name: string, d: unknown) => handlers.get(name)?.forEach((h) => h(d));
const whenChanged = changesOn(on);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

test("a run's end reads the files' kind again, once for several close together; one still going reads nothing", async () => {
  let files = 0;
  let lists = 0;
  const offFiles = whenChanged(['files'], () => files++);
  const offLists = whenChanged(['playlists'], () => lists++);
  emit('file_run', { state: 'writing' });
  await sleep(400);
  assert.equal(files, 0);
  emit('file_run', { state: 'done' });
  emit('file_run', { state: 'done' });
  await sleep(400);
  assert.equal(files, 1);
  assert.equal(lists, 0);
  emit('changed', { area: 'playlists' });
  await sleep(400);
  assert.deepEqual([files, lists], [1, 1]);
  offFiles();
  emit('file_run', { state: 'done' });
  await sleep(400);
  assert.equal(files, 1, 'no longer once let go');
  offLists();
});

test("a change of words (decision 0097) reads what shows words again, and nothing else", async () => {
  let words = 0;
  let files = 0;
  const offWords = whenChanged(['words'], () => words++);
  const offFiles = whenChanged(['files'], () => files++);
  emit('changed', { area: 'words' });
  await sleep(400);
  assert.deepEqual([words, files], [1, 0]);
  offWords();
  offFiles();
});


test('a steady stream of changes reads at most once a second, and always once more after its last (M7: a busy intake)', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 1_000_000 });
  try {
    const reads: number[] = [];
    const off = whenChanged(['incoming'], () => reads.push(Date.now()));
    let lastChange = 0;
    for (let i = 0; i < 40; i++) {
      lastChange = Date.now();
      emit('changed', { area: 'incoming' });
      mock.timers.tick(250);
    }
    mock.timers.tick(5000);
    off();
    assert.ok(reads.length >= 9 && reads.length <= 11, `10 s of changes, about ten reads: ${reads.length}`);
    for (let i = 1; i < reads.length; i++) assert.ok(reads[i] - reads[i - 1] >= 1000, `a second apart: ${reads[i] - reads[i - 1]} ms`);
    assert.ok(reads[reads.length - 1] > lastChange, 'a read after the last change');
    assert.equal(reads[0] - 1_000_000, 250, 'the first as soon as before');
  } finally {
    mock.timers.reset();
  }
});
