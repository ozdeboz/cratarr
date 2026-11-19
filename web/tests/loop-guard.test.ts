// The owner, 2026-11-19 ("that needs to be fixed durably and robustly"): a page that asks one thing over and over is held
// back before the server. F39's effect read what it wrote and asked /api/artists/… thousands of times a minute; the server,
// throttling the page, refused its covers too. The review of 1.2 (M7): a legitimate burst must never be held and leave the
// page stale, and a real loop must still be.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { LOOP_GAP, LOOP_MAX, LOOP_MS, loopHeld } from '../src/lib/loopGuard.ts';

/** Asks `path` at each time, the ones that went out. */
const sent = (path: string, times: number[]) => times.filter((t) => !loopHeld(path, undefined, t));
const every = (from: number, gap: number, until: number) => Array.from({ length: Math.floor((until - from) / gap) }, (_, i) => from + i * gap);

test('a GET asked more than LOOP_MAX times in LOOP_MS is held back, and only that one', () => {
  const t0 = 1_000_000;
  for (let i = 0; i < LOOP_MAX; i++) assert.equal(loopHeld('/api/artists/about?name=a', undefined, t0 + i), false);
  assert.equal(loopHeld('/api/artists/about?name=a', undefined, t0 + LOOP_MAX), true, 'the one over is held');
  assert.equal(loopHeld('/api/artists/about?name=b', undefined, t0 + LOOP_MAX), false, 'another path is not');
  assert.equal(loopHeld('/api/artists/about?name=a', undefined, t0 + LOOP_MS + LOOP_MAX + 1), false, 'a window later it goes again');
});

test('a poll every 2 s and every write are never held', () => {
  const t0 = 5_000_000;
  for (let i = 0; i < 100; i++) assert.equal(loopHeld('/api/state', undefined, t0 + i * 2000), false);
  for (let i = 0; i < 100; i++) assert.equal(loopHeld('/api/plays', { method: 'POST' }, t0 + i), false);
});

test('a busy intake: a read every 250 ms for 30 s, then the one after it, all go out (M7)', () => {
  const t0 = 9_000_000;
  const burst = [...every(t0, 250, t0 + 30_000), t0 + 31_000];
  assert.deepEqual(sent('/api/library/records', burst), burst, 'nothing held, the read after the burst least of all');
});

test('a loop is held for as long as it asks, at 90 a second or at 8: only LOOP_MAX go out in a minute', () => {
  const t0 = 13_000_000;
  const fast = sent('/api/artists/about?name=loop', every(t0, 11, t0 + 60_000));
  assert.equal(fast.length, LOOP_MAX, 'F39: about 90 a second');
  assert.ok(fast[fast.length - 1] - t0 < 1000, 'held within a second');
  assert.equal(sent('/api/crates/c1', every(t0, 120, t0 + 60_000)).length, LOOP_MAX, 'a slower loop, one ask per answer');
  assert.ok(LOOP_GAP < 1000, 'a loop asks again sooner than whenChanged reads (a second apart)');
});

test('held asks do not count: once a loop stops, the next ask goes out when what went out has left the window', () => {
  const t0 = 17_000_000;
  const path = '/api/crates/c2';
  assert.equal(sent(path, every(t0, 11, t0 + 3000)).length, LOOP_MAX);
  // Stopped. A view reads again a second later, and each second: held while LOOP_MAX that went out are in the window …
  const after = every(t0 + 4000, 1000, t0 + 20_000);
  const out = sent(path, after);
  // … and out again from the moment the loop's first asks are older than the window, though the held ones are not.
  assert.equal(out[0], after.find((t) => t - t0 >= LOOP_MS));
  assert.deepEqual(out, after.filter((t) => t >= out[0]));
});

test('every JSON request goes through the guard', () => {
  const api = readFileSync(new URL('../src/lib/api.ts', import.meta.url), 'utf8');
  assert.match(api, /async function fetchJson<T>\(path: string, init\?: RequestInit\): Promise<T> \{\s*if \(loopHeld\(path, init\)\) throw/);
});
