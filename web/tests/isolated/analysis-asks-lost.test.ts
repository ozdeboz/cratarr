// A lost ask for a track's analysis fails in time and lets the ones behind it go (0.16.0, 2026-11-08: one request lost to a
// network that dropped held every waveform after it, on the tablet and the ThinkPad, until the page was opened again).
// lib/analysisAsks, the real one, over a network that never answers the first.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { askAnalysis, limits, WAVE, NEXT } = await import('../../src/lib/analysisAsks.ts');

test('an ask that never answers fails after its time, and the next is asked', async () => {
  limits.askMs = 40;
  const asked: string[] = [];
  globalThis.fetch = ((url: string, init?: RequestInit) => {
    asked.push(url);
    if (url === '/lost') {
      return new Promise((_, reject) => init?.signal?.addEventListener('abort', () => reject(init.signal!.reason)));
    }
    return Promise.resolve(new Response('ok'));
  }) as typeof fetch;
  const lost = askAnalysis('/lost', WAVE);
  const next = askAnalysis('/next', NEXT);
  await assert.rejects(lost, (e: Error) => e.name === 'TimeoutError', 'the lost one fails');
  assert.equal(await (await next).text(), 'ok', 'the one behind it is asked and answered');
  assert.deepEqual(asked, ['/lost', '/next']);
});
