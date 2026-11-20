// Health's Missing music (docs/records-and-copies.md): rows as Health has them, a library read-only, a track with a way to say what
// it is now, asked first as it is for good. The page reads the source: it takes a whole app to run.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = (file: string) => readFileSync(new URL(`../src/lib/${file}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test('a track of Missing music opens to its reconnecting, and nothing else on Health does', () => {
  // F28.2: what can be done for an item is one piece (lib/HealthItemTools) by one when-table (lib/healthTools), Health's row and a
  // record's sheet alike; Health keeps no tools of its own.
  const tools = source('HealthItemTools.svelte');
  assert.ok(tools.includes("{#if has('reconnect')}<div class=\"tool\"><HealthReconnect track={x.track!} title={x.title} ongone={ongone} /></div>{/if}"));
  assert.equal(tools.match(/<HealthReconnect /g)?.length, 1);
  assert.ok(source('healthTools.ts').includes("['reconnect', (p, x) => p === 'library:missing' && !!x.track],"));
  assert.equal(source('HealthPanel.svelte').match(/<HealthReconnect /g), null);
});

test('a row says since when, as the finding does', () => {
  assert.ok(source('HealthPanel.svelte').includes('x.since ? `since ${since(x.since)}` : null'));
});

test('a found track is asked about, as it is for good, and the report is read again once done', () => {
  const tool = source('HealthReconnect.svelte');
  assert.ok(tool.includes('onclick={() => (asking = t)}'), 'a tap asks');
  assert.ok(tool.includes('<ConfirmSheet') && tool.includes('onconfirm={() => reconnect(t)}'), 'by the sheet every question has');
  assert.ok(tool.includes('await reconnectTrack(track, t.id);\n      ongone();'));
});

test('the library is searched by what the track was, the words free to change', () => {
  const tool = source('HealthReconnect.svelte');
  assert.ok(tool.includes('let query = $state(untrack(() => title));'));
  assert.ok(tool.includes('<input bind:value={query}'));
});

test('it asks the server to take it', () => {
  assert.ok(source('api.ts').includes("post<unknown>('/api/missing/reconnect', { track, to })"));
});
