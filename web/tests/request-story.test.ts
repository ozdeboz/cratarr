// A request's sheet tells its whole story (RQ01; the owner, preview 1's F4: "a lot more could be displayed"): where it stands,
// each fetcher's part, what was set aside and why, when it was asked and delivered and where, in the sheet's own pieces; users
// read all of it on their own requests, only administrators act (lib/requestStory, lib/RequestsPanel). RQ01.8 (d), RQ01.9 and
// RQ01.10 of SCENARIOS-P1-FIX; the stories are in the server's form (`GET /api/requests/{id}`).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import type { RequestStory, StorySoulseek } from '../src/lib/api.ts';
import type { StoryItem, StoryRow, StorySheet } from '../src/lib/requestStory.ts';

const { requestStory, storyAgainIn, tickOf } = await import('../src/lib/requestStory.ts');
const { standing, standingTone } = await import('../src/lib/requestState.ts');
const { folderLine, itemLine } = await import('../src/lib/soulseekRules.ts');
const { roomLine } = await import('../src/lib/room.ts');
const { when } = await import('../src/lib/settings/when.ts');

const NOW = 1_800_000_000;
const DAY = 86_400;
const at = (t: number) => `at ${t}`;

// alice's request 7 of record al-42 (Glass Harbour, "Night Ferry"), as the row is in /api/requests.
const ROW = {
  id: 7,
  user: 'alice',
  album_key: 'night ferry',
  artist_key: 'glass harbour',
  release_group: null,
  album: 'Night Ferry',
  artist: 'Glass Harbour',
  year: 2019,
  cover: null,
  at: NOW - 3 * DAY,
  status: 'approved',
  reason: null,
  decided_by: null,
  decided_at: null,
  fetched: null,
  hand_over_again: false,
  record: 42,
};
const story = (state: Record<string, unknown>, extra: Record<string, unknown> = {}) =>
  ({ ...ROW, state, album_state: { state: 'looking' }, opens: 'al-42', asked_at: NOW - 3 * DAY, ...extra }) as unknown as RequestStory;

// Soulseek's part (RQ01.4): peer-a's FLAC with a free slot, peer-b's MP3 in a queue; peer-c's folder set aside, file by file;
// peer-d failed it.
const peerA = { user: 'peer-a', folder: 'Night Ferry', format: 'flac', lossless: true, free_slot: true, queue: 0, speed: 2_100_000, extra: 0 };
const peerB = { user: 'peer-b', folder: 'Night Ferry (2019)', format: 'mp3', lossless: false, free_slot: false, queue: 4, speed: 0, extra: 1 };
const asideC = { user: 'peer-c', files: [{ file: '01 Slack Water.flac', why: 'Clipping at 1:23' }, { file: '02 Neap.flac', why: 'Too short' }] };
const slsk = (over: Partial<StorySoulseek>): StorySoulseek => ({ id: 'ss-1', state: 'waiting', sources: [], failed: ['peer-d'], misses: 0, set_aside: [asideC], off: false, ...over });
const wanted = (soulseek?: Record<string, unknown>, lidarr?: Record<string, unknown>) => ({ state: 'wanted', ...(soulseek ? { soulseek } : {}), ...(lidarr ? { lidarr } : {}) });

const found = story(wanted({ state: 'found' }), { soulseek: slsk({ state: 'found', sources: [peerA, peerB] }) });
const searching = story(wanted({ state: 'searching' }), { soulseek: slsk({ state: 'searching' }) });
const approved = story(wanted(), { soulseek: slsk({ state: 'approved', sources: [peerA, peerB], chosen: 1 }) });
const fetching = story(wanted({ state: 'fetching', percent: 37 }), { soulseek: slsk({ state: 'fetching', percent: 37, sources: [peerA, peerB], chosen: 0 }) });
const checking = story(wanted({ state: 'fetching', percent: 100 }), { soulseek: slsk({ state: 'checking', percent: 100, sources: [peerA], chosen: 0 }) });
const held = story(wanted(), { soulseek: slsk({ state: 'approved', sources: [peerA, peerB], chosen: 0, held: { why: 'slskd could not be reached', until: NOW + 300 } }) });
const checkHeld = story(wanted({ state: 'fetching', percent: 100 }), {
  soulseek: slsk({ state: 'checking', percent: 100, sources: [peerA], chosen: 0, held: { why: 'AcoustID could not be asked', until: NOW + 600 } }),
});
const waiting = story(wanted({ state: 'waiting' }), { soulseek: slsk({ state: 'waiting', misses: 3, next_search: NOW + DAY }) });
const stuck = story(wanted({ state: 'stuck', why: 'MusicBrainz has no list of it' }), { soulseek: slsk({ state: 'stuck', stuck: 'MusicBrainz has no list of it' }) });
const placed = story({ state: 'arrived' }, { soulseek: slsk({ state: 'placed', sources: [peerA], chosen: 0 }), where: { library: 4, name: 'Soulseek Incoming' } });
// slskd switched off (staging): the item as kept, nothing moves.
const offFound = story(wanted({ state: 'found' }), { soulseek: slsk({ state: 'found', sources: [peerA, peerB], off: true }) });
const offWaiting = story(wanted({ state: 'waiting' }), { soulseek: slsk({ state: 'waiting', misses: 1, off: true }) });
const offFetching = story(wanted({ state: 'fetching', percent: 37 }), { soulseek: slsk({ state: 'fetching', percent: 37, sources: [peerA], chosen: 0, off: true }) });

// Lidarr's part (RQ01.3): its steps; its own words only in an administrator's story (W6).
const SAID = 'Import failed: /media/music not found on http://192.168.1.20:8686';
const looking = story(wanted(undefined, { state: 'looking' }), { lidarr: { step: 'looking' } });
const grabbed = story(wanted(undefined, { state: 'fetching', done: 0.42 }), { lidarr: { step: 'grabbed', done: 0.42 } });
const importing = story(wanted(undefined, { state: 'fetching', done: 1 }), { lidarr: { step: 'importing' } });
const imported = story({ state: 'arrived' }, { lidarr: { step: 'imported' }, where: { library: 1, name: 'Music' } });
const lidarrStuck = story(wanted(undefined, { state: 'stuck' }), { lidarr: { step: 'stuck' } });
const lidarrStuckRoot = story(wanted(undefined, { state: 'stuck' }), { lidarr: { step: 'stuck', said: SAID } });
const notHanded = story(wanted(), { lidarr: { step: 'not_handed' } });
const notHandedRoot = story(wanted(), { fetched: 'Not handed over to Lidarr: it answered 500', lidarr: { step: 'not_handed', said: 'Not handed over to Lidarr: it answered 500' } });
const handOver = story(wanted(), { hand_over_again: true });
const both = story(wanted({ state: 'fetching', percent: 60 }, { state: 'fetching', done: 0.42 }), {
  lidarr: { step: 'grabbed', done: 0.42 },
  soulseek: slsk({ state: 'fetching', percent: 60, sources: [peerA, peerB], chosen: 1 }),
});

// Why it waits; asked and delivered, when and where (RQ01.6); what changed since (RQ01.7).
const forRoom = story({ state: 'waiting_for_room', used: 130e9, room: 100e9 });
const forSpace = story({ state: 'waiting_for_space' });
const delivered = story({ state: 'delivered' }, { status: 'delivered', decided_at: NOW - DAY, delivered_at: NOW - DAY, where: { library: 1, name: 'Music' } });
const ownMusic = story({ state: 'done' }, { where: { library: 9, name: 'Own music' } });
const takenBack = story({ state: 'cancelled' }, { status: 'cancelled' });
const declined = story({ state: 'declined' }, { status: 'declined', reason: 'Not on vinyl' });
const pending = story({ state: 'waiting_for_approval' }, { status: 'pending' });
const noRecord = story(wanted(), { opens: null });

// PB4 (the review of preview 1's fixes), with Lidarr connected: a Want that waits for its asker's room, for space or for its
// album's release (no fetcher was to have it yet); one Soulseek alone looks for (Lidarr not its fetcher: the server gives no
// Lidarr part); one that arrived, Lidarr told to stop.
const DAY_OUT = '2026-12-04';
const STOPPED = 'Lidarr is looking for it; Lidarr told to stop: it arrived';
const lidarrRoom = story({ state: 'waiting_for_room', used: 130e9, room: 100e9 }, { lidarr: { step: 'waits_for_room' } });
const lidarrSpace = story({ state: 'waiting_for_space' }, { lidarr: { step: 'waits_for_space' } });
const lidarrRelease = story(wanted({ state: 'waiting' }), { lidarr: { step: 'waits_for_release', day: DAY_OUT }, soulseek: slsk({ state: 'waiting', next_search: NOW + 16 * DAY }) });
const soulseekAlone = story(wanted({ state: 'searching' }), { soulseek: slsk({ state: 'searching' }) });
const stopped = story({ state: 'arrived' }, { lidarr: { step: 'stopped' }, soulseek: slsk({ state: 'placed', sources: [peerA], chosen: 0 }), where: { library: 4, name: 'Soulseek Incoming' } });
const stoppedRoot = story({ state: 'arrived' }, { fetched: STOPPED, lidarr: { step: 'stopped', said: STOPPED }, where: { library: 4, name: 'Soulseek Incoming' } });

const SOULSEEK = { found, searching, approved, fetching, checking, held, checkHeld, waiting, stuck, placed, offFound, offWaiting, offFetching, both };
const ALL: Record<string, RequestStory> = {
  ...SOULSEEK,
  looking,
  grabbed,
  importing,
  imported,
  lidarrStuck,
  lidarrStuckRoot,
  notHanded,
  notHandedRoot,
  handOver,
  forRoom,
  forSpace,
  delivered,
  ownMusic,
  takenBack,
  declined,
  pending,
  noRecord,
  lidarrRoom,
  lidarrSpace,
  lidarrRelease,
  stopped,
  stoppedRoot,
};

const groupOf = (sheet: StorySheet, title: string) => sheet.groups.find((g) => g.title === title);
const isItem = (x: StoryRow): x is StoryItem => !('choice' in x);
const items = (sheet: StorySheet, title: string) => (groupOf(sheet, title)?.rows ?? []).filter(isItem);
/** A row's words as it reads: its name, then its line. */
const words = (x: StoryItem) => [x.name, x.line].filter(Boolean).join(' · ');

// The same item as the Soulseek list has it (/api/soulseek), its record named: what itemLine says of it there.
const asListItem = (p: StorySoulseek) => ({
  id: p.id,
  record: { record: 42, artist: 'Glass Harbour', album: 'Night Ferry', year: 2019, request: 7 },
  state: p.state,
  folders: p.sources.map((x) => ({ ...x, dir: `@@a\\Music\\Glass Harbour\\${x.folder}` })),
  chosen: p.chosen ?? null,
  failed: p.failed,
  misses: p.misses,
  due: p.held?.until ?? p.next_search ?? 0,
  why: p.held?.why ?? p.stuck ?? null,
  percent: p.percent ?? 0,
  set_aside: p.set_aside.map((a) => ({ user: a.user, path: `/data/soulseek-set-aside/ss-1 ${a.user}`, why: Object.fromEntries(a.files.map((f) => [f.file, f.why])) })),
});
const HEAD = 'Glass Harbour · 2019 · ';
function listWords(p: StorySoulseek): string {
  const line = itemLine(asListItem(p), NOW);
  assert.ok(line.startsWith(HEAD), line);
  return line.slice(HEAD.length);
}
const sourceLine = (x: typeof peerA) => folderLine({ ...x, dir: x.folder, user: '' });

// --- RQ01.8 (d): read again while it shows ---

test('a story is read again every 3 s while Soulseek searches, is to fetch, fetches or checks, and is not held', () => {
  for (const [name, s] of Object.entries({ searching, approved, fetching, checking, both })) assert.equal(storyAgainIn(s, NOW), 3000, name);
  const holdOver = story(wanted(), { soulseek: slsk({ state: 'approved', sources: [peerA], chosen: 0, held: { why: 'slskd could not be reached', until: NOW - 1 } }) });
  assert.equal(storyAgainIn(holdOver, NOW), 3000, 'a hold whose time has come is under way again');
});

test('else every 30 s: found, waiting, stuck, placed, held, Lidarr alone, a closed one, slskd switched off', () => {
  const still = { found, waiting, stuck, placed, held, checkHeld, offFound, offWaiting, offFetching, looking, grabbed, importing, forRoom, forSpace, delivered, takenBack, pending };
  for (const [name, s] of Object.entries(still)) assert.equal(storyAgainIn(s, NOW), 30000, name);
  assert.equal(storyAgainIn(held, NOW + 301), 3000, 'held until a time now past');
});

// The panel's use of them, read from the source, as the sheet does not run here.
const panel = readFileSync(new URL('../src/lib/RequestsPanel.svelte', import.meta.url), 'utf8');
const api = readFileSync(new URL('../src/lib/api.ts', import.meta.url), 'utf8');
const sheet = panel.slice(panel.indexOf('{#if opened}'), panel.indexOf('{#if slskOpen}'));

test('the sheet reads its story when it opens, again by storyAgainIn while it shows, and no more once it closes', () => {
  assert.ok(api.includes('export const getRequestStory = (id: number) => quietly<RequestStory>(`/api/requests/${id}`);'));
  assert.match(
    panel,
    /\$effect\(\(\) => \{\s*const r = opened;\s*const gen = \+\+storyGen;\s*story = null;\s*storyError = null;\s*storyPick = null;\s*if \(r && readsStory\(r\)\) readStory\(r\.id, gen\);\s*return \(\) => \(storyGen\+\+, clearTimeout\(storyTimer\)\);\s*\}\);/,
  );
  assert.match(panel, /function readStory\(id: number, gen: number\) \{\s*clearTimeout\(storyTimer\);\s*getRequestStory\(id\)\.then\(\s*\(s\) => \{\s*if \(gen !== storyGen\) return;/);
  assert.match(panel, /storyTimer = setTimeout\(\(\) => readStory\(id, gen\), storyAgainIn\(s, storyNow\)\);/);
  assert.match(panel, /const readsStory = \(r: AlbumRequest\) => me\.admin \|\| r\.user === me\.name;/, "its asker's and the administrators', as the server answers it");
});

// --- RQ01.9: one state, one word ---

test("the sheet's head is the row's own words, standing(view), in every story, for a user and for an administrator", () => {
  for (const [name, s] of Object.entries(ALL))
    for (const admin of [false, true]) {
      const sheet = requestStory(s, admin, NOW);
      assert.equal(sheet.head, standing(s, when), name);
      assert.equal(sheet.tone, standingTone(s), name);
      assert.equal(requestStory(s, admin, NOW, at).head, standing(s, at), name);
    }
});

test("the Soulseek group's line is itemLine's words without the artist and year", () => {
  for (const [name, s] of Object.entries(SOULSEEK))
    for (const admin of [false, true]) {
      const first = groupOf(requestStory(s, admin, NOW), 'Soulseek')?.rows[0];
      assert.ok(first && isItem(first), name);
      assert.equal(first.name, listWords(s.soulseek!), name);
      assert.ok(!first.name.includes('Glass Harbour') && !first.name.includes('2019'), name);
    }
});

test("the room line is roomLine's, and the disk floor's wait the row's words", () => {
  const head = requestStory(forRoom, false, NOW).head;
  assert.equal(head, `Waits for room · ${roomLine(130e9, 100)}`);
  assert.equal(head, 'Waits for room · 130 of 100 GB');
  assert.equal(requestStory(forSpace, false, NOW).head, 'Waits: the server is short of space');
  for (const s of [forRoom, forSpace]) assert.ok(!JSON.stringify(requestStory(s, false, NOW).groups).includes('GB'), 'said once, in the head');
});

test("asked, delivered and where are the story's own: when, as the page says a time, and the library's name", () => {
  const asked = (s: RequestStory, said = at) => items(requestStory(s, false, NOW, said), 'Asked').map((x) => [x.name, x.value]);
  assert.deepEqual(asked(delivered), [['Asked', `at ${NOW - 3 * DAY}`], ['Delivered', `at ${NOW - DAY}`], ['Where', 'Music']]);
  assert.deepEqual(asked(delivered, when), [['Asked', when(NOW - 3 * DAY)], ['Delivered', when(NOW - DAY)], ['Where', 'Music']]);
  assert.deepEqual(asked(placed), [['Asked', `at ${NOW - 3 * DAY}`], ['Where', 'Soulseek Incoming']]);
  assert.deepEqual(asked(ownMusic), [['Asked', `at ${NOW - 3 * DAY}`], ['Where', 'Own music']]);
  assert.deepEqual(asked(forRoom), [['Asked', `at ${NOW - 3 * DAY}`]], 'no copy of its asker\'s: no where');
});

test("Lidarr's own words are the administrators', beside Stuck and Not handed over; a user's sheet never shows them", () => {
  const lidarrWords = (s: RequestStory, admin: boolean) => items(requestStory(s, admin, NOW), 'Lidarr').map(words);
  assert.deepEqual(lidarrWords(lidarrStuckRoot, true), [`Stuck · ${SAID}`]);
  assert.deepEqual(lidarrWords(notHandedRoot, true), ['Not handed over · Not handed over to Lidarr: it answered 500']);
  assert.deepEqual(lidarrWords(lidarrStuckRoot, false), ['Stuck'], 'even where they came');
  const json = JSON.stringify(requestStory(lidarrStuckRoot, false, NOW));
  assert.ok(!json.includes('192.168.1.20') && !json.includes('/media/music'), json);
});

test("no state's word is worked out again in the page: the sheet's words come from the shared rules", () => {
  const code = readFileSync(new URL('../src/lib/requestStory.ts', import.meta.url), 'utf8')
    .split(/\r?\n/)
    .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
    .join('\n');
  assert.match(code, /import \{[^}]*\bstanding\b[^}]*\} from '\.\/requestState';/);
  assert.match(code, /import \{[^}]*\bitemLine\b[^}]*\bfolderLine\b[^}]*\} from '\.\/soulseekRules';|import \{[^}]*\bfolderLine\b[^}]*\bitemLine\b[^}]*\} from '\.\/soulseekRules';/);
  const theirs = ['Being fetched', 'Waits for room', 'short of space', 'Taken back', 'Partly there', 'In the library', 'Not found', 'To be fetched', 'Checking its files', 'In Incoming', 'Fetching ·', 'again in', 'again tomorrow', 'free slot', 'MB/s', 'extra file', 'set aside'];
  for (const w of theirs) assert.ok(!code.includes(w), `"${w}" is said by lib/requestState or lib/soulseekRules`);
});

// --- RQ01.10: what users and administrators get ---

test('groups in order Lidarr, Soulseek, Set aside, Asked, each only where it has something', () => {
  const titles = (s: RequestStory, admin = false) => requestStory(s, admin, NOW).groups.map((g) => g.title);
  assert.deepEqual(titles(both), ['Lidarr', 'Soulseek', 'Set aside', 'Asked']);
  assert.deepEqual(titles(both, true), ['Lidarr', 'Soulseek', 'Set aside', 'Asked']);
  assert.deepEqual(titles(looking), ['Lidarr', 'Asked']);
  assert.deepEqual(titles(story(wanted({ state: 'found' }), { soulseek: slsk({ state: 'found', sources: [peerA], set_aside: [] }) })), ['Soulseek', 'Asked']);
  for (const s of [forRoom, forSpace, takenBack, declined, delivered]) assert.deepEqual(titles(s), ['Asked']);
  for (const [name, s] of Object.entries(ALL)) for (const g of requestStory(s, true, NOW).groups) assert.ok(g.rows.length, `${name}: ${g.title}`);
});

test("Lidarr's part: looking, grabbed with its fill, importing, imported, stuck, not handed over", () => {
  const one = (s: RequestStory) => {
    const xs = items(requestStory(s, false, NOW), 'Lidarr');
    assert.equal(xs.length, 1);
    return xs[0];
  };
  assert.equal(words(one(looking)), 'Looking for it');
  assert.equal(words(one(grabbed)), 'Grabbed · 42%');
  assert.equal(one(grabbed).progress, 0.42);
  assert.equal(words(one(importing)), 'Importing');
  assert.equal(words(one(imported)), 'Imported');
  assert.equal(words(one(lidarrStuck)), 'Stuck');
  assert.equal(words(one(notHanded)), 'Not handed over');
  for (const s of [looking, importing, imported, lidarrStuck, notHanded]) assert.ok(one(s).progress == null);
});

// PB4: "Not handed over" only where Lidarr should have had it and didn't.
test("the Lidarr line says what is true: a wait for room, space or its release says so, in the row's words; told to stop says why", () => {
  const line = (s: RequestStory, admin = false) => items(requestStory(s, admin, NOW), 'Lidarr').map(words);
  for (const admin of [false, true]) {
    assert.deepEqual(line(lidarrRoom, admin), ['Waits for room']);
    assert.deepEqual(line(lidarrSpace, admin), ['Waits: the server is short of space']);
    assert.deepEqual(line(lidarrRelease, admin), [`Waits for its release · ${DAY_OUT}`]);
    assert.deepEqual(line(stopped, admin), ['Told to stop: it arrived']);
    for (const s of [lidarrRoom, lidarrSpace, lidarrRelease, stopped, stoppedRoot]) assert.ok(!line(s, admin).some((w) => w.includes('Not handed over')), `${admin}: ${line(s, admin)}`);
  }
  assert.deepEqual(line(stoppedRoot, true), [`Told to stop: it arrived · ${STOPPED}`], "Lidarr's own words beside it, an administrator's");
  assert.deepEqual(line(stoppedRoot, false), ['Told to stop: it arrived']);
  // One state, one word (RQ01.9): the Lidarr line's wait is the row's own.
  assert.ok(standing(lidarrRoom, when).startsWith(`${line(lidarrRoom)[0]} · `), standing(lidarrRoom, when));
  assert.equal(line(lidarrSpace)[0], standing(lidarrSpace, when));
  assert.ok(!JSON.stringify(requestStory(lidarrRoom, false, NOW).groups).includes('GB'), 'the room\'s numbers said once, in the head');
  // Soulseek's alone: no Lidarr group.
  assert.deepEqual(requestStory(soulseekAlone, true, NOW).groups.map((g) => g.title), ['Soulseek', 'Set aside', 'Asked']);
});

test('nothing in the Lidarr group is red unless something failed: stuck, or not handed over', () => {
  const tone = (s: RequestStory, admin: boolean) => items(requestStory(s, admin, NOW), 'Lidarr')[0]?.tone ?? null;
  for (const admin of [false, true]) {
    for (const [name, s] of Object.entries({ lidarrRoom, lidarrSpace, lidarrRelease, stopped, stoppedRoot, looking, grabbed, importing, imported }))
      assert.equal(tone(s, admin), null, name);
    for (const [name, s] of Object.entries({ lidarrStuck, notHanded, notHandedRoot })) assert.equal(tone(s, admin), 'bad', name);
  }
});

test('Soulseek: each source named by its peer with its folderLine, the one fetching with its fill; held, stuck and failed peers', () => {
  const rows = (s: RequestStory, admin = false) => items(requestStory(s, admin, NOW), 'Soulseek');
  assert.deepEqual(rows(found).slice(1).map((x) => [x.name, x.line]), [['peer-a', sourceLine(peerA)], ['peer-b', sourceLine(peerB)], ['Failed', 'peer-d']]);
  assert.equal(sourceLine(peerA), 'FLAC · free slot · 2.1 MB/s');
  assert.equal(sourceLine(peerB), 'MP3 · queue 4 · 1 extra file');
  assert.ok(rows(found).every((x) => x.progress == null), 'nothing fetched yet');
  const f = rows(fetching);
  assert.deepEqual(f.map((x) => x.progress ?? null), [null, 0.37, null, null], 'peer-a, the one fetching, with its fill');
  assert.deepEqual(rows(both).map((x) => x.progress ?? null), [null, null, 0.6, null], 'the one chosen');
  assert.equal(rows(held)[0].name, 'slskd could not be reached · again within the hour · 1 folder set aside');
  assert.equal(rows(checkHeld)[0].name, 'AcoustID could not be asked · again within the hour · 1 folder set aside');
  assert.equal(rows(waiting)[0].name, 'Not found · 3 times · again tomorrow · 1 folder set aside');
  assert.equal(rows(stuck)[0].name, 'MusicBrainz has no list of it · 1 folder set aside');
  assert.deepEqual(rows(story(wanted({ state: 'found' }), { soulseek: slsk({ state: 'found', sources: [peerA], failed: [] }) })).map((x) => x.name), [listWords(slsk({ state: 'found', sources: [peerA], failed: [] })), 'peer-a'], 'no failed peers, no row of them');
});

test('Set aside: each folder named by its peer, "file: why · file: why", and never its path on the server', () => {
  for (const admin of [false, true])
    assert.deepEqual(
      items(requestStory(found, admin, NOW), 'Set aside').map((x) => [x.name, x.line]),
      [['peer-c', '01 Slack Water.flac: Clipping at 1:23 · 02 Neap.flac: Too short']],
    );
  // A path the server never sends, sent anyway: still not shown.
  const leaky = { ...found, soulseek: { ...found.soulseek!, set_aside: [{ ...asideC, path: '/data/soulseek-set-aside/ss-1 peer-c' }] } } as RequestStory;
  for (const admin of [false, true]) {
    const json = JSON.stringify(requestStory(leaky, admin, NOW));
    assert.ok(!json.includes('soulseek-set-aside') && !json.includes('/data/'), json);
  }
});

test("a user's actions are View and Take back only: no tick, no Search now, no Hand over again", () => {
  assert.deepEqual(requestStory(found, false, NOW).actions, ['view', 'takeBack']);
  for (const [name, s] of Object.entries(ALL)) {
    const sheet = requestStory(s, false, NOW);
    assert.ok(sheet.actions.every((a) => a === 'view' || a === 'takeBack'), `${name}: ${sheet.actions}`);
    assert.ok(sheet.groups.every((g) => g.rows.every(isItem)), `${name}: no choice of sources`);
  }
  assert.deepEqual(requestStory(waiting, false, NOW).actions, ['view', 'takeBack']);
  assert.deepEqual(requestStory(handOver, false, NOW).actions, ['view', 'takeBack']);
  assert.deepEqual(requestStory(pending, false, NOW).actions, ['view', 'takeBack'], "0.16's waiting one, hers to take back");
  assert.deepEqual(requestStory(takenBack, false, NOW).actions, ['view'], 'taken back: no action but View');
  assert.deepEqual(requestStory(delivered, false, NOW).actions, ['view']);
  assert.deepEqual(requestStory(noRecord, false, NOW).actions, ['takeBack'], 'no record, no View');
});

test("an administrator's: a choice of the sources and Fetch where Found, Search now where waiting or stuck, Hand over again, View, Take back", () => {
  const root = (s: RequestStory) => requestStory(s, true, NOW);
  const a = root(found);
  assert.ok(a.actions.includes('fetch') && a.actions.includes('searchNow'), `${a.actions}`);
  assert.deepEqual(a.actions, ['fetch', 'searchNow', 'view', 'takeBack']);
  const rows = groupOf(a, 'Soulseek')!.rows;
  const choices = rows.filter((x) => !isItem(x));
  assert.deepEqual(choices, [
    {
      choice: {
        label: 'Source',
        options: [
          { value: '0', label: 'peer-a', line: sourceLine(peerA) },
          { value: '1', label: 'peer-b', line: sourceLine(peerB) },
        ],
        chosen: '0',
      },
    },
  ]);
  assert.ok(!rows.filter(isItem).some((x) => x.name.startsWith('peer-')), 'the choice in place of the sources');
  assert.deepEqual(root(waiting).actions, ['searchNow', 'view', 'takeBack']);
  assert.deepEqual(root(stuck).actions, ['searchNow', 'view', 'takeBack']);
  for (const s of [fetching, approved, checking, held, searching]) assert.deepEqual(root(s).actions, ['view', 'takeBack'], 'Search now never while it moves');
  assert.deepEqual(root(handOver).actions, ['handOverAgain', 'view', 'takeBack']);
  assert.deepEqual(root(takenBack).actions, ['view'], 'taken back: no action but View');
  // slskd switched off: no tick and no Search now offered.
  assert.deepEqual(root(offFound).actions, ['view', 'takeBack']);
  assert.deepEqual(root(offWaiting).actions, ['view', 'takeBack']);
  assert.ok(groupOf(root(offFound), 'Soulseek')!.rows.every(isItem), 'no choice while nothing moves');
  assert.deepEqual(items(root(offFound), 'Soulseek').slice(1, 3).map((x) => x.name), ['peer-a', 'peer-b']);
});

test("Fetch sends the source picked by its place, its user and its folder's name (RQ01.8 (b))", () => {
  assert.deepEqual(tickOf(found.soulseek!, 1), { id: 'ss-1', folder: 1, user: 'peer-b', folder_name: 'Night Ferry (2019)' });
  assert.deepEqual(tickOf(found.soulseek!, null), { id: 'ss-1', folder: 0, user: 'peer-a', folder_name: 'Night Ferry' }, 'none picked: the one chosen, else the first');
  assert.deepEqual(tickOf(slsk({ id: 'ss-9', state: 'found', sources: [peerA, peerB], chosen: 1 }), null), { id: 'ss-9', folder: 1, user: 'peer-b', folder_name: 'Night Ferry (2019)' });
  assert.equal(tickOf(found.soulseek!, 5), null);
  assert.match(api, /approveSoulseek = \(items: \{ id: string; folder\?: number; user\?: string; folder_name\?: string \}\[\]\)/);
  assert.match(panel, /const tick = s\.soulseek && tickOf\(s\.soulseek, storyPick\);\s*if \(tick\) void onSoulseek\(s, \(\) => approveSoulseek\(\[tick\]\), "It wasn't fetched"\);/);
});

test("Search now and the tick name the story's own Soulseek item, by its id, with no other lookup", () => {
  assert.match(panel, /function searchStory\(s: RequestStory\) \{\s*const id = s\.soulseek\?\.id;\s*if \(id\) void onSoulseek\(s, \(\) => soulseekNow\(id\), "It isn't being searched"\);\s*\}/);
  assert.match(sheet, /<MenuEntry icon="search" label="Search now" onclick=\{\(\) => searchStory\(s\)\} \/>/);
  assert.ok(!panel.includes('getSoulseek'), "Soulseek's list is not read for it");
});

test("the sheet renders requestStory with the sheet's own pieces only: MenuSheet, Group, Item, MenuEntry, ChoiceList (and the Notice)", () => {
  assert.ok(sheet.length > 100, 'the opened sheet is there');
  assert.match(sheet, /\{@const st = story\?\.id === r\.id \? requestStory\(story, me\.admin, storyNow\) : null\}/);
  const tags = [...new Set([...sheet.replace(/<!--[\s\S]*?-->/g, '').matchAll(/<([A-Za-z][\w-]*)/g)].map((m) => m[1]))].sort();
  assert.deepEqual(tags, ['ChoiceList', 'Group', 'Item', 'MenuEntry', 'MenuSheet', 'Notice', 'div']);
  assert.deepEqual(sheet.match(/<div\b[^>]*>/g), ['<div role="menu">']);
  assert.match(sheet, /\{#each st\.groups as g \(g\.title\)\}\s*<Group title=\{g\.title\}>/);
  assert.match(sheet, /<ChoiceList label=\{x\.choice\.label\} options=\{x\.choice\.options\}/);
  // Out of reach (or until it is read): the row's own line, and the existing Notice.
  assert.match(sheet, /<Group><Item name=\{st\?\.head \?\? standing\(r, when\)\} lead=\{st\?\.tone \?\? standingTone\(r\)\} \/><\/Group>/);
  assert.match(sheet, /<Notice text=\{storyError\} onretry=\{\(\) => readStory\(r\.id, storyGen\)\} away="Requests need Cratarr\." \/>/);
  // Its actions are the story's; no Cancel, no Take it anyway, no check's progress (preview 2).
  assert.match(sheet, /\{#each st\.actions as a \(a\)\}/);
  for (const w of ['Cancel', 'Take it anyway', 'Discover']) assert.ok(!sheet.includes(`label="${w}`), w);
});
