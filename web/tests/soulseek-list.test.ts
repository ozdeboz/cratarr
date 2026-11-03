// Soulseek in Cratarr on the Requests page (decision 0099 and its amendment): what a record's row says, what can be
// ticked and what Fetch sends (lib/soulseekRules); the fetchers' lists, siblings, each where it is connected.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { approvePayload, folderLine, itemLine, searchable, tickable } = await import('../src/lib/soulseekRules.ts');

const folder = (user: string, lossless = true) => ({ user, dir: `Music\\${user}\\Late Set`, format: lossless ? 'flac' : 'mp3', lossless, free_slot: true, queue: 0, speed: 2_500_000, extra: 0 });
const item = (id: string, state: string, extra: Record<string, unknown> = {}) =>
  ({
    id,
    record: { artist: 'M. Rivers', album: 'Late Set', release_group: 'rg', year: 1998 },
    request: 7,
    state,
    folders: state === 'found' ? [folder('ann'), folder('bo', false)] : [],
    chosen: state === 'found' ? 0 : null,
    failed: [],
    misses: 0,
    due: 0,
    percent: 0,
    set_aside: [],
    ...extra,
  }) as Parameters<typeof itemLine>[0];

test("a record's row says where it stands", () => {
  assert.equal(folderLine(folder('ann')), 'FLAC · free slot · 2.5 MB/s · ann');
  assert.equal(itemLine(item('1', 'found')), 'M. Rivers · 1998 · FLAC · free slot · 2.5 MB/s · ann');
  assert.equal(itemLine(item('1', 'waiting')), 'M. Rivers · 1998 · Waiting to be searched');
  assert.equal(itemLine(item('1', 'waiting', { misses: 3, due: 1_000 + 86_400 }), 1_000), 'M. Rivers · 1998 · Not found · 3 times · again tomorrow');
  assert.equal(itemLine(item('1', 'fetching', { percent: 42.4 })), 'M. Rivers · 1998 · Fetching · 42%');
  assert.equal(itemLine(item('1', 'stuck', { why: 'No release to judge it by', set_aside: [{ user: 'x', path: 'p', why: {} }] })), 'M. Rivers · 1998 · No release to judge it by · 1 folder set aside');
});

test('only found records are ticked; Fetch sends the ticked, each with its picked folder', () => {
  const items = [item('a', 'found'), item('b', 'found'), item('c', 'waiting')];
  assert.deepEqual(items.map(tickable), [true, true, false]);
  assert.deepEqual(items.map(searchable), [false, false, true]);
  assert.deepEqual(approvePayload(items, new Set(['a', 'b', 'c']), { b: 1 }), [{ id: 'a' }, { id: 'b', folder: 1 }]);
});

test("the fetchers' lists show where each is connected; Soulseek's to administrators", async () => {
  const { listsShown } = await import('../src/lib/requestLists.ts');
  const ids = (answers: boolean, admin: boolean, fetchers: string[]) => listsShown(answers, admin, fetchers).map((l: { id: string }) => l.id);
  assert.deepEqual(ids(true, true, ['lidarr', 'soulseek']), ['waiting', 'mine', 'lidarr', 'soulseek']);
  assert.deepEqual(ids(true, true, ['soulseek']), ['waiting', 'mine', 'soulseek'], 'Soulseek without Lidarr');
  assert.deepEqual(ids(false, false, ['lidarr', 'soulseek']), ['mine', 'lidarr']);
  assert.deepEqual(ids(true, true, []), ['waiting', 'mine']);
});
