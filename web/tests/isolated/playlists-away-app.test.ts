// The playlists with Cratarr out of reach (0.17, the owner: every page answers from the device where it can): their list and a
// playlist's rows through the read door (lib/api), from the device's copy (the app's here: lib/deviceAnswers on a stand-in for the
// app's end), and the shell's state keeps them so (lib/live). The real modules.
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
const bridge: { postMessage(m: string): void; onmessage: ((e: { data: string }) => void) | null } = { postMessage() {}, onmessage: null };
g.CratarrApp = bridge;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });
localStorage.setItem('cratarr-who', '"ann"');

const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json' } });
const MIX = { id: 'pl-1', name: 'Mix', member: false, count: 2, dead: 0, links: [], spoken: [], mine: true };
const ROWS = { source: 'cratarr', rows: [{ entry: 'A/b.flac', file: 'b.flac', id: 'tr-3', name: 'Two' }] };
/** What the app's copy says, by what is asked of it; null: it holds nothing. */
let device: ((ask: { ask: string; id?: string }) => unknown) | null = null;
const asked: string[] = [];
globalThis.fetch = (async (input: string) => {
  const url = String(input);
  asked.push(url);
  if (url.startsWith('/__device/answer')) {
    const said = device?.(JSON.parse(decodeURIComponent(url.slice(url.indexOf('ask=') + 4))));
    return said == null ? new Response('', { status: 404 }) : json(said);
  }
  throw new TypeError('no network');
}) as typeof fetch;

await import('../../src/lib/away.svelte.ts');
const { playlistRows, readPlaylists } = await import('../../src/lib/api.ts');
const { live, refresh } = await import('../../src/lib/live.svelte.ts');
const fromApp = (m: Record<string, unknown>) => bridge.onmessage!({ data: JSON.stringify(m) });

beforeEach(() => {
  asked.length = 0;
  device = (a) => (a.ask === 'playlists' ? [MIX] : a.ask === 'playlist_rows' && a.id === 'pl-1' ? ROWS : null);
  fromApp({ type: 'network', online: false, validated: false, kind: 'none' });
});

test('out of reach, the playlists are the device’s, asked of nobody else', async () => {
  assert.deepEqual(await readPlaylists(), [MIX]);
  assert.deepEqual(asked.filter((a) => a.startsWith('/api')), []);
});

test('a playlist’s rows are the tracks the device holds of it', async () => {
  assert.deepEqual(await playlistRows('pl-1'), ROWS);
  assert.ok(asked.some((a) => a.includes(encodeURIComponent('"playlist_rows"'))));
});

test('the shell shows the device’s playlists, with no error to say', async () => {
  await refresh();
  assert.deepEqual(live.view?.playlists, [MIX]);
  assert.equal(live.error, null);
});

test('a device that holds nothing leaves it to Cratarr, which is out of reach: the error stands', async () => {
  device = null;
  live.view = null;
  await refresh();
  assert.equal(live.view, null);
  assert.ok(live.error);
});
