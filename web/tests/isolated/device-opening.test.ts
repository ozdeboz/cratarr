// The page's loader says what it waits for while the library on this device opens for an answer (the owner, 2026-11-08: the
// first answer with Cratarr away took 4 to 5 s on the tablet; "a standard one, with the progress explaining what it is"). In the
// app: from its first answer until Cratarr answers again, when the app lets its copy go. lib/deviceAnswers, the real one.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

// The app's door, as android's WebView puts it on the page.
(globalThis as Record<string, unknown>).window = globalThis;
(globalThis as Record<string, unknown>).CratarrApp = { postMessage() {}, onmessage: null };
const app = await import('../../src/lib/app.svelte.ts');
const { deviceAsk, deviceOpening } = await import('../../src/lib/deviceAnswers.svelte.ts');
const { whenReached } = await import('../../src/lib/reach.svelte.ts');

assert.ok(app.inApp, 'run as the app');

test('cold, the loader says the library opens; warm, nothing; Cratarr back, cold again', async () => {
  let release!: () => void;
  globalThis.fetch = (() => new Promise<Response>((r) => (release = () => r(new Response('[]'))))) as typeof fetch;
  const first = deviceAsk({ ask: 'crates' });
  await Promise.resolve();
  assert.equal(deviceOpening.asks, 1, 'the first answer opens it');
  release();
  await first;
  assert.equal(deviceOpening.asks, 0);
  const warm = deviceAsk({ ask: 'crates' });
  await Promise.resolve();
  assert.equal(deviceOpening.asks, 0, 'open already');
  release();
  await warm;
  for (const f of whenReached) f();
  const again = deviceAsk({ ask: 'crates' });
  await Promise.resolve();
  assert.equal(deviceOpening.asks, 1, 'let go when Cratarr answered: opened again');
  release();
  await again;
});

test('the loader shows the line only where nothing else goes under it', () => {
  const loader = readFileSync(new URL('../../src/lib/PageLoading.svelte', import.meta.url), 'utf8');
  assert.match(loader, /\{#if children\}.*\{:else if deviceOpening\.asks > 0\}.*Opening the library on this device/);
});

test('the count is told untracked, so an ask made in an effect never runs that effect again', () => {
  const src = readFileSync(new URL('../../src/lib/deviceAnswers.svelte.ts', import.meta.url), 'utf8');
  assert.match(src, /untrack\(\(\) => deviceOpening\.asks\+\+\)/);
  assert.match(src, /untrack\(\(\) => deviceOpening\.asks--\)/);
  assert.doesNotMatch(src, /^\s*deviceOpening\.asks(\+\+|--);/m, 'never counted where an effect would track it');
});
