// The page's asks of the player that plays, as the server's PlayerCommand (lib/player/commands; 0.10, one protocol):
// the same command the op made before, field for field, and which player the page drives.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { commandOf, holdsQueue, savedOutput, outputOfTarget, targetOfOutput } = await import('../src/lib/player/commands.ts');
const t = { source: 'cratarr', id: 'tr-1', name: 'A', artist: null, album: null, album_id: null };

test('the ops that are named otherwise', () => {
  assert.deepEqual(commandOf('queue', { tracks: [t], index: 2, positionMs: 1500, play: true, shuffled: false, name: 'Evening' }), {
    command: 'play',
    tracks: [t],
    index: 2,
    position_ms: 1500,
    name: 'Evening',
  });
  assert.deepEqual(commandOf('queue', { tracks: [t], index: 0, positionMs: 0, name: null }), { command: 'play', tracks: [t], index: 0, position_ms: 0 });
  assert.deepEqual(commandOf('seek', { positionMs: 9000, wind: true }), { command: 'seek', position_ms: 9000, wind: true });
  assert.deepEqual(commandOf('seek', { positionMs: 9000 }), { command: 'seek', position_ms: 9000 });
  assert.deepEqual(commandOf('play'), { command: 'resume' });
  assert.deepEqual(commandOf('sleep', { atMs: 123, endOfTrack: false }), { command: 'sleep', at_ms: 123 });
  assert.deepEqual(commandOf('sleep', { atMs: null, endOfTrack: true }), { command: 'sleep', at_ms: null, end_of_track: true });
});

test('the ops that are the command by name, with their fields as given', () => {
  assert.deepEqual(commandOf('add', { at: 3, tracks: [t] }), { command: 'add', tracks: [t], at: 3 });
  assert.deepEqual(commandOf('add', { at: 3, tracks: [t], after: 'e1' }), { command: 'add', tracks: [t], at: 3, after: 'e1' });
  assert.deepEqual(commandOf('move', { from: 1, to: 4, entry: 'e2', after: 'e3' }), { command: 'move', from: 1, to: 4, entry: 'e2', after: 'e3' });
  assert.deepEqual(commandOf('remove', { index: 2 }), { command: 'remove', index: 2 });
  assert.deepEqual(commandOf('jump', { index: 5, entry: 'e5' }), { command: 'jump', index: 5, entry: 'e5' });
  assert.deepEqual(commandOf('upcoming', { tracks: [t], shuffled: true }), { command: 'upcoming', tracks: [t] });
  assert.deepEqual(commandOf('shuffle', { on: true }), { command: 'shuffle', on: true });
  assert.deepEqual(commandOf('skip_favourites', { on: false }), { command: 'skip_favourites', on: false });
  assert.deepEqual(commandOf('repeat', { mode: 'all' }), { command: 'repeat', mode: 'all' });
  assert.deepEqual(commandOf('radio', { on: true, by: 'artist', count: 2 }), { command: 'radio', on: true, by: 'artist', count: 2 });
  assert.deepEqual(commandOf('autoplay', { mode: 'stop' }), { command: 'autoplay', mode: 'stop' });
  assert.deepEqual(commandOf('autoplay', { mode: 'similar', albums: false, restart: false, seed: ['a'], within: '' }), {
    command: 'autoplay',
    mode: 'similar',
    albums: false,
    restart: false,
    seed: ['a'],
    within: '',
  });
  assert.deepEqual(commandOf('wind', { dir: 'back', phase: 'start' }), { command: 'wind', dir: 'back', phase: 'start' });
  assert.deepEqual(commandOf('direction', { dir: 'L' }), { command: 'direction', dir: 'L' });
  for (const op of ['toggle', 'next', 'previous', 'stop']) assert.deepEqual(commandOf(op, op === 'toggle' ? { plain: true } : {}), { command: op });
  assert.equal(commandOf('nonsense'), null);
});

test('which player the page drives', () => {
  assert.equal(holdsQueue('local', true, null), true, 'here');
  assert.equal(holdsQueue('local', false, null), false, 'a browser that cannot play here');
  assert.equal(holdsQueue('player:Tablet', false, 'Tablet'), true, "a player of Cratarr's it drives");
});

test("the media server's player is gone (0.12): a kept one plays here, an app pointed there is pointed here", () => {
  assert.equal(savedOutput('remote'), 'local', 'kept from before 0.12');
  assert.equal(savedOutput('local'), 'local');
  assert.equal(savedOutput('player:Salon tablet'), 'player:Salon tablet');
  assert.equal(savedOutput('player:'), null, 'no name');
  assert.equal(savedOutput(42), null);
  assert.equal(savedOutput(undefined), null);
  assert.equal(outputOfTarget('server'), 'local', 'an app still pointed at the server');
  assert.equal(outputOfTarget('self'), 'local');
  assert.equal(outputOfTarget('player:Tablet'), 'player:Tablet');
  assert.equal(targetOfOutput('local'), 'self');
  assert.equal(targetOfOutput('player:Tablet'), 'player:Tablet');
});
