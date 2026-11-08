// A move between two of Cratarr's players goes by its own handover, so every screen showing the one it left follows the music
// (the owner, 2026-11-08: This device chosen on the phone, and the tablet and the ThinkPad said nothing played, where showing the
// phone would be "seamless"). lib/handoverEnds and lib/output's move, the real ones.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { handoverEnds } = await import('../../src/lib/handoverEnds.ts');
const players = [
  { name: 'ThinkPad', online: true },
  { name: 'Phone', online: true },
  { name: 'Salon tablet', online: false },
];

test('from the player driven or this device, to the one chosen or this device, both connected', () => {
  assert.deepEqual(handoverEnds('local', 'ThinkPad', false, 'Phone', players), { from: 'ThinkPad', to: 'Phone' }, 'This device on the phone');
  assert.deepEqual(handoverEnds('player:ThinkPad', null, true, 'Phone', players), { from: 'Phone', to: 'ThinkPad' }, 'from here to a player');
  assert.equal(handoverEnds('player:Salon tablet', 'ThinkPad', false, 'Phone', players), null, 'one not connected: the page’s way');
  assert.equal(handoverEnds('local', 'ThinkPad', false, null, players), null, 'this device no player of Cratarr’s');
  assert.equal(handoverEnds('player:Phone', null, true, 'Phone', players), null, 'to itself');
  assert.equal(handoverEnds('local', null, false, 'Phone', players), null, 'nothing plays anywhere it drives');
});

test('a move asks the handover first, and the page’s own way only where it is refused or no handover fits', () => {
  const src = readFileSync(new URL('../../src/lib/output.svelte.ts', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
  const move = src.slice(src.indexOf('export async function moveTo('), src.indexOf('export const hasMusic'));
  assert.ok(move.includes("await playerCommand(ends.from, { command: 'handover', to: ends.to });"));
  assert.ok(move.indexOf("command: 'handover'") < move.indexOf('await port().leave(was);'), 'before the stop here');
  assert.match(move, /follow\(output\.startsWith\('player:'\) \? ends\.to : null\);\s*return;/);
});
