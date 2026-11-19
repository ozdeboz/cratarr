// The owner, 2026-11-19 ("that needs to be fixed durably and robustly"): a page that asks one thing over and over is held
// back before the server. F39's effect read what it wrote and asked /api/artists/… thousands of times a minute; the server,
// throttling the page, refused its covers too.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { LOOP_MAX, LOOP_MS, loopHeld } from '../src/lib/loopGuard.ts';

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

test('every JSON request goes through the guard', () => {
  const api = readFileSync(new URL('../src/lib/api.ts', import.meta.url), 'utf8');
  assert.match(api, /async function fetchJson<T>\(path: string, init\?: RequestInit\): Promise<T> \{\s*if \(loopHeld\(path, init\)\) throw/);
});
