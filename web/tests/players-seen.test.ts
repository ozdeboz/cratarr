// One word for an offline player (OBJECT-MAP-2 §2.11, LANES v2 §3.4): Settings → Devices said "Offline" on a player's row and
// "Never" on its own page's Last seen, for one state (not online, never seen). Both say Offline, the word the app says for a device
// that is not online: Devices in Account and in Users, and Diagnostics' players. The page takes a whole app, so its source is read.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { lastSeen, when } = await import('../src/lib/settings/when.ts');
const source = (file: string) => readFileSync(new URL(`../src/${file}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test('a player not online and never seen says Offline, on its row and on its own page', () => {
  assert.equal(lastSeen(false, 0), 'Offline');
  assert.equal(lastSeen(true, 0), 'Online');
  assert.equal(lastSeen(true, 1_700_000_000), 'Online');
  assert.equal(lastSeen(false, 1_700_000_000), when(1_700_000_000));
  const page = source('lib/PlayersSettings.svelte');
  assert.ok(!page.includes("'Never'"), 'no second word for it');
  assert.ok(page.includes('<Item name="Last seen" value={lastSeen(p.online, p.seen)} />'), 'its page reads the one rule');
  assert.match(page, /const seen = \(p: PlayOnPlayer\) => [^\n]*lastSeen\(p\.online, p\.seen\)/, 'and so does its row');
});

test("Offline is the rest of the app's word for a device not online", () => {
  assert.ok(source('lib/settings/DiagnosticsPart.svelte').includes(": 'Offline'"), "Diagnostics' players");
  for (const f of ['lib/settings/AccountPart.svelte', 'lib/settings/UsersPart.svelte']) assert.ok(source(f).includes(": 'offline'"), `${f}'s Devices`);
});
