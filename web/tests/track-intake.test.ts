// Health's many-at-once review list (0.13, mockup D; "Bulk search, you approve"): each track's line by where it
// stands, only found ones tickable, Add sending the ticked ones each with its copy (picked, else chosen), when the
// list is asked again, and ticks kept only where they still can be.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { approvePayload, anyOver, copyOf, foundIds, itemEnd, itemLine, itemTone, keptTicks, lookedFor, tickable, underWay } = await import('../src/lib/trackIntake.ts');

const copy = (id: string, lossless = true) => ({
  id,
  user: 'someone',
  file: `Music\\${id}.flac`,
  format: lossless ? 'FLAC' : 'MP3',
  lossless,
  bitrate: lossless ? null : 320,
  sampleRate: lossless ? 44100 : null,
  bitDepth: lossless ? 16 : null,
  size: 31457280,
  lengthMs: 252000,
  freeSlot: true,
  queue: null,
  speed: null,
});
type State = 'queued' | 'searching' | 'found' | 'none' | 'failed' | 'approved' | 'fetching' | 'checking' | 'added' | 'refused';
const item = (id: string, state: State, extra: Record<string, unknown> = {}) => ({
  id,
  album: { source: 'cratarr', id: 'a1', title: 'Late Set', artist: 'M. Rivers' },
  disc: 1,
  position: 4,
  title: 'Harbour Lights',
  recording: 'r1',
  state,
  candidates: state === 'found' ? [copy('c1'), copy('c2', false)] : [],
  chosen: state === 'found' ? 'c1' : null,
  ...extra,
});

test('each state says where it stands; a found one its chosen copy, or the one picked', () => {
  assert.equal(itemLine(item('1', 'queued')), 'Waiting to be looked for');
  assert.equal(itemLine(item('1', 'searching')), 'Looking for it');
  assert.equal(itemLine(item('1', 'found')), 'FLAC 16/44.1 · 30 MB · free slot · someone');
  assert.equal(itemLine(item('1', 'found'), 'c2'), 'MP3 320 kbps · 30 MB · free slot · someone');
  assert.equal(itemLine(item('1', 'none')), 'Not found');
  assert.equal(itemLine(item('1', 'failed')), 'The search did not finish');
  assert.equal(itemLine(item('1', 'approved')), 'To be added');
  assert.equal(itemLine(item('1', 'fetching')), 'Downloading');
  assert.equal(itemLine(item('1', 'checking')), 'Checking it');
  assert.equal(itemLine(item('1', 'added')), 'Added');
  assert.equal(itemLine(item('1', 'refused', { why: 'It is 14 s longer than the recording' })), 'It is 14 s longer than the recording');
  assert.equal(itemEnd(item('1', 'refused', { why: 'x' })), 'Refused');
  assert.equal(itemEnd(item('1', 'failed')), 'Failed');
  assert.equal(itemEnd(item('1', 'found')), null);
  assert.equal(itemTone(item('1', 'refused')), 'bad');
  assert.equal(itemTone(item('1', 'none')), 'warn');
  assert.equal(itemTone(item('1', 'found')), null);
});

test('only a found track with a copy can be ticked; Tick all found takes those', () => {
  assert.equal(tickable(item('1', 'found')), true);
  assert.equal(tickable(item('1', 'found', { candidates: [] })), false);
  assert.equal(tickable(item('1', 'added')), false);
  const items = [item('1', 'found'), item('2', 'none'), item('3', 'found'), item('4', 'searching')];
  assert.deepEqual(foundIds(items), ['1', '3']);
});

test('Add sends only the ticked found ones, each with its picked copy, else its chosen one', () => {
  const items = [item('1', 'found'), item('2', 'found'), item('3', 'none'), item('4', 'found')];
  const ticked = new Set(['1', '2', '3']);
  assert.deepEqual(approvePayload(items, ticked, { 2: 'c2' }), [
    { id: '1', candidate: 'c1' },
    { id: '2', candidate: 'c2' },
  ]);
  // Nothing ticked, nothing sent.
  assert.deepEqual(approvePayload(items, new Set(), {}), []);
  // A pick no longer among its copies falls back to the chosen one.
  assert.equal(copyOf(item('1', 'found'), 'gone')?.id, 'c1');
});

test('asked again while it runs or a track is on its way in; Clear finished where any is over', () => {
  assert.equal(underWay(null), false);
  assert.equal(underWay({ running: true, items: [] }), true);
  assert.equal(underWay({ running: false, items: [item('1', 'found'), item('2', 'added')] }), false);
  assert.equal(underWay({ running: false, items: [item('1', 'checking')] }), true);
  assert.equal(anyOver([item('1', 'found')]), false);
  assert.equal(anyOver([item('1', 'found'), item('2', 'refused')]), true);
  assert.deepEqual(lookedFor([item('1', 'found'), item('2', 'searching'), item('3', 'queued'), item('4', 'none')]), { done: 2, total: 4 });
});

test('a tick is kept only while its track can still be ticked', () => {
  const items = [item('1', 'found'), item('2', 'approved')];
  assert.deepEqual([...keptTicks(new Set(['1', '2', '9']), items)], ['1']);
});
