// The one page cache (SCENARIOS-DC1 LV2, LV4–LV7, LV9, HD13, RS18–RS22, CN10, CN11, RV4; DOMAIN-CORE §13): every record the page
// shows is one entry of @tanstack/query-core's cache under ['record', id, epoch], made stale by what the server tells
// (`changed { rev, ids }`, the change log's `record` kind), read again while on screen. The server is a stand-in here: it answers
// each record's entity as GET /api/records/{id} does, its epoch naming the place's libraries as `library_set` counts them.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';
import type { ActionView } from '../../src/lib/generated/ActionView';
import type { Version } from '../../src/lib/generated/Version';
import type { Section } from '../../src/lib/generated/Section';

const { RecordCache, look, KEEP_MS, recordKey, onChanged } = await import('../../src/lib/recordCache.ts');
const { playRow, sectionsOf, versionLines, Runs } = await import('../../src/lib/sheet/record.ts');
type Entity = import('../../src/lib/recordCache.ts').RecordEntity;
type Place = import('../../src/lib/recordCache.ts').Place;
type Shown = import('../../src/lib/recordCache.ts').Shown;

/** Places whose libraries follow the switch (LC10's `library_set`, the server's); the others count every library. */
const FOLLOWS = new Set(['library', 'artist', 'search', 'label', 'playlist', 'crate']);
const follows = (p: Place) => FOLLOWS.has(p.from) || (p.from === 'page' && FOLLOWS.has(p.opener ?? 'library'));
/** The stand-in's epoch: the viewer, and the libraries off where the place follows the switch (LV4). */
let viewer = 'alice|User|Full|request,offline,playlists';
const epochOf = (p: Place) => `${viewer}|${follows(p) ? [...(p.without ?? [])].sort().join(',') : ''}`;

function act(name: string, section: Section, more: Partial<ActionView> = {}): ActionView {
  return { name, title: name, icon: 'play', group: section === 'play' ? 'play' : 'library', section, method: 'POST', href: `/api/x/${name}`, ...more };
}
function copy(id: string, library: string, quality: string, plays: boolean): Version {
  return { kind: 'copy', id, record: 'al-42', library, quality, plays, ...(plays ? {} : { action: 'play_from' }) };
}
type Facts = { rev: number; words?: string; at?: number; is?: Entity['state']['is']; actions?: ActionView[]; versions?: Version[]; sections?: Section[] };
function entity(id: string, place: Place, f: Facts): Entity {
  const actions = f.actions ?? [];
  return {
    class: 'record',
    id,
    rev: f.rev,
    epoch: epochOf(place),
    properties: {
      type: { value: 'Album', by: [] },
      title: { value: 'Night Ferry', by: [] },
      artists: { value: [{ id: 'ar-1', name: 'Glass Harbour' }], by: [] },
      credit: 'Glass Harbour',
      year: { value: 2019, by: [] },
      labels: { value: [], by: [] },
      cover: { value: null, by: [] },
    },
    state: { is: f.is ?? 'had', words: f.words ?? 'In your library', ...(f.at ? { at: f.at } : {}) },
    actions,
    sections: f.sections ?? (['head', 'play', 'versions', 'library', 'go'] as Section[]).filter((s) => s === 'head' || (s === 'versions' ? (f.versions ?? []).length > 1 : actions.some((a) => a.section === s))),
    entities: f.versions ?? [],
    links: [],
  };
}

/** The stand-in server: what it answers now per id (a function of the place), and every ask, held until answered. */
function server() {
  const asks: { id: string; place: Place; ok: (e: Entity) => void; no: (e: unknown) => void }[] = [];
  const now = new Map<string, (place: Place) => Entity | { gone: string }>();
  let auto = true;
  const cache = new RecordCache(
    (id, place) =>
      new Promise<Entity>((ok, no) => {
        asks.push({ id, place, ok, no });
        if (auto) queueMicrotask(() => answer(asks.length - 1));
      }),
    (e) => (e instanceof Error && e.message.startsWith('404 ') ? e.message.slice(4) : null),
  );
  function answer(i: number, given?: Entity) {
    const a = asks[i];
    const r = given ?? now.get(a.id)?.(a.place);
    if (!r) return a.no(new Error('500 none'));
    if ('gone' in r) a.no(new Error(`404 ${r.gone}`));
    else a.ok(r);
  }
  return {
    cache,
    asks,
    now,
    answer,
    hold: () => (auto = false),
    release: () => (auto = true),
  };
}
const settle = async () => {
  for (let i = 0; i < 20; i++) await new Promise((go) => setImmediate(go));
};
/** A watcher: what it shows now. */
function shown(cache: InstanceType<typeof RecordCache>, id: string, place: Place) {
  const seen: { last: Shown | null } = { last: null };
  const stop = cache.watch(id, place, (s) => (seen.last = s));
  return { seen, stop, get: () => seen.last! };
}

const MUSIC_IN = (rev: number) => (place: Place) =>
  entity('al-42', place, { rev, actions: [act('play', 'play'), act('play_from', 'library', { fields: { from: 'IN' } })], versions: [copy('MUSIC', 'MUSIC', 'MP3 320', true), copy('IN', 'IN', 'FLAC', false)] });
const MUSIC_ONLY = (rev: number) => (place: Place) => entity('al-42', place, { rev, actions: [act('play', 'play')], versions: [] });

test('one record, one entry: a card, a row, a page and the sheet change together', async () => {
  const s = server();
  s.now.set('al-42', MUSIC_IN(100));
  const off = ['cratarr/more'];
  const places: Place[] = [
    { from: 'library', without: off },
    { from: 'artist', without: off },
    { from: 'search', without: off },
    { from: 'page', opener: 'artist', without: off },
  ];
  const four = places.map((p) => shown(s.cache, 'al-42', p));
  await settle();
  for (const w of four) assert.equal(w.get().entity?.rev, 100);
  const before = s.asks.length;
  s.now.set('al-42', MUSIC_ONLY(140));
  s.cache.told({ rev: 140, ids: [{ id: 'al-42', op: 'changed' }] });
  await settle();
  assert.equal(s.asks.length - before, 1, 'one request went for al-42 (LG01 never counts more)');
  for (const w of four) {
    assert.equal(w.get().entity?.rev, 140, 'all four show the new state together');
    assert.equal(w.get().current, true);
  }
  const keys = s.cache.client.getQueryCache().findAll({ queryKey: ['record', 'al-42'], type: 'active' }).map((q) => JSON.stringify(q.queryKey));
  assert.deepEqual([...new Set(keys)], [JSON.stringify(recordKey('al-42', epochOf(places[0])))], 'one key: [record, id, epoch]');
  for (const w of four) w.stop();
});

test('a copy deleted after the read is never kept', async () => {
  const s = server();
  s.now.set('al-42', MUSIC_IN(100));
  const w = shown(s.cache, 'al-42', { from: 'library' });
  await settle();
  assert.deepEqual(versionLines(w.get().entity!).map((l) => l.text), ['MUSIC · MP3 320 · plays', 'IN · FLAC']);
  s.hold();
  s.cache.told({ rev: 140, ids: [{ id: 'al-42', op: 'changed' }] });
  await settle();
  assert.equal(w.get().current, false, '100 < 140: the cached al-42 is stale');
  assert.equal(look(w.get(), KEEP_MS), 'loader', 'a stale object is never shown as the truth past 300 ms');
  s.answer(s.asks.length - 1, MUSIC_ONLY(140)({ from: 'library' }));
  await settle();
  assert.equal(w.get().current, true);
  assert.deepEqual(versionLines(w.get().entity!), [], 'MUSIC alone: the deleted copy is gone from screen');
  w.stop();
});

test('on changed, what is on screen is read now and the rest marked; a late answer is not kept as current', async () => {
  const s = server();
  s.now.set('al-42', MUSIC_IN(100));
  s.now.set('al-9', (p) => ({ ...entity('al-9', p, { rev: 90 }) }));
  const off9 = shown(s.cache, 'al-9', { from: 'library' });
  await settle();
  off9.stop();
  const w = shown(s.cache, 'al-42', { from: 'library' });
  await settle();
  s.hold();
  const before = s.asks.length;
  s.cache.told({ rev: 130, ids: [{ id: 'al-42', op: 'changed' }, { id: 'al-9', op: 'changed' }] });
  await settle();
  assert.deepEqual(s.asks.slice(before).map((a) => a.id), ['al-42'], 'al-42 read now; al-9 only marked');
  const nine = s.cache.client.getQueryCache().findAll({ queryKey: ['record', 'al-9'] });
  assert.ok(nine.length && nine.every((q) => q.state.isInvalidated), 'al-9 is marked stale, read when shown');
  // An answer read at 120 lands after the change at 130: kept, not current, and read once more.
  s.answer(before, MUSIC_IN(120)({ from: 'library' }));
  await settle();
  assert.equal(w.get().current, false, 'the late answer at 120 is not kept as current');
  assert.equal(s.asks.length, before + 2, 'read again for 130');
  s.answer(before + 1, MUSIC_IN(130)({ from: 'library' }));
  await settle();
  assert.equal(w.get().current, true);
  const again = shown(s.cache, 'al-9', { from: 'library' });
  await settle();
  assert.equal(s.asks.at(-1)?.id, 'al-9', 'al-9 read once it shows');
  again.stop();
  w.stop();
});

test('what is no row goes in the epoch and the key: the switch re-keys what follows it, a right every record', async () => {
  const s = server();
  s.now.set('al-42', MUSIC_IN(100));
  const where = ['artist', 'crate', 'playlist', 'work'] as const;
  let off: string[] = [];
  const watchers = where.map((from) => ({ from, w: shown(s.cache, 'al-42', { from, without: off }) }));
  await settle();
  const keyOf = (from: string) => JSON.stringify(recordKey('al-42', epochOf({ from, without: off })));
  const keysBefore = Object.fromEntries(where.map((f) => [f, keyOf(f)]));
  // MUSIC switched off: each place asks with the switch as it stands now.
  off = ['cratarr/music'];
  for (const x of watchers) x.w.stop();
  const again = where.map((from) => ({ from, w: shown(s.cache, 'al-42', { from, without: off }) }));
  await settle();
  for (const x of again) {
    const e = x.w.get().entity!;
    const moved = keyOf(x.from) !== keysBefore[x.from];
    assert.equal(e.epoch, epochOf({ from: x.from, without: off }));
    assert.equal(moved, x.from !== 'work', `${x.from}: ${x.from === 'work' ? 'a work counts every library: same key' : 'follows the switch: a new key'}`);
  }
  // A right changed (`changed { area: "account" }`): every epoch of the viewer anew, read again.
  viewer = 'alice|User|Full|request,playlists';
  const asked = s.asks.length;
  s.cache.account();
  await settle();
  assert.ok(s.asks.length > asked, 'read again');
  for (const x of again) assert.equal(x.w.get().entity?.epoch, epochOf({ from: x.from, without: off }), `${x.from} re-keyed`);
  for (const x of again) x.w.stop();
  viewer = 'alice|User|Full|request,offline,playlists';
});

test('the account signal reads me again and re-keys every record (CN10, M13)', async () => {
  const s = server();
  let me = 0;
  onChanged(s.cache, { area: 'account' }, () => void me++);
  assert.equal(me, 1, '/api/me read again');
  onChanged(s.cache, { area: 'playlists' }, () => void me++);
  assert.equal(me, 1, 'another area is not the cache\'s');
});

test('a copy gone while shown', async () => {
  const s = server();
  s.now.set('al-42', MUSIC_ONLY(100));
  const w = shown(s.cache, 'al-42', { from: 'artist' });
  await settle();
  assert.equal(playRow(w.get().entity!).length, 1);
  s.now.set('al-42', (p) => entity('al-42', p, { rev: 150, is: 'retired', words: 'Was in MUSIC until', at: 1_700_000_000 }));
  s.cache.told({ rev: 150, ids: [{ id: 'al-42', op: 'changed' }] });
  await settle();
  const e = w.get().entity!;
  assert.equal(e.state.words, 'Was in MUSIC until');
  assert.deepEqual(playRow(e), [], 'no Play row: Play is never offered on a copy gone');
  assert.ok(!sectionsOf(e).some((x) => x.section === 'library'), 'no Library section');
  w.stop();
});

test('a sheet follows its record into the one it went into', async () => {
  const s = server();
  s.now.set('al-21', (p) => entity('al-21', p, { rev: 100 }));
  s.now.set('al-20', (p) =>
    entity('al-20', p, { rev: 160, actions: [act('play', 'play'), act('play_from', 'library', { fields: { from: 'IN' } })], versions: [{ ...copy('MUSIC', 'MUSIC', 'MP3 320', true), record: 'al-20' }, { ...copy('IN', 'IN', 'FLAC', false), record: 'al-20' }] }),
  );
  const w = shown(s.cache, 'al-21', { from: 'library' });
  await settle();
  s.cache.told({ rev: 160, ids: [{ id: 'al-21', op: 'merged', into: 'al-20' }, { id: 'al-20', op: 'changed' }] });
  await settle();
  const e = w.get().entity!;
  assert.equal(e.id, 'al-20', 'RS01 reads al-20 and shows it');
  assert.equal(w.get().id, 'al-20');
  assert.deepEqual(versionLines(e).map((l) => l.text), ['MUSIC · MP3 320 · plays', 'IN · FLAC']);
  assert.equal(versionLines(e)[1].play?.fields?.from, 'IN', "a tap on IN's line acts on al-20's IN copy");
  assert.equal(s.cache.client.getQueryCache().findAll({ queryKey: ['record', 'al-21'] }).length, 0, 'no entry left on a gone id');
  w.stop();
});

test('an id merged before it was asked answers as the one it went into, and the sheet re-keys to it', async () => {
  const s = server();
  s.now.set('al-9', (p) => entity('al-4', p, { rev: 100 }));
  const w = shown(s.cache, 'al-9', { from: 'search' });
  await settle();
  assert.equal(w.get().id, 'al-4');
  assert.equal(w.get().entity?.id, 'al-4');
  w.stop();
});

test('a record gone answers in the server\'s words, and the cache drops it', async () => {
  const s = server();
  s.now.set('al-77', () => ({ gone: 'That record is gone.' }));
  const w = shown(s.cache, 'al-77', { from: 'library' });
  await settle();
  assert.equal(w.get().gone, 'That record is gone.');
  assert.equal(w.get().entity, null);
  assert.equal(look(w.get(), 0), 'gone', 'the existing notice, never a blank sheet');
  w.stop();
  assert.equal(s.cache.client.getQueryCache().findAll({ queryKey: ['record', 'al-77'] }).length, 0, 'no forgotten id kept');
});

test('an action refused reads its record again, and what it offered leaves', async () => {
  const s = server();
  s.now.set('al-42', (p) => entity('al-42', p, { rev: 100, actions: [act('replace_in', 'library')] }));
  const w = shown(s.cache, 'al-42', { from: 'library' });
  await settle();
  s.now.set('al-42', (p) => entity('al-42', p, { rev: 101 }));
  // 409 "That copy is gone" (RS19), or 403 a right taken away (CN10): the server's words in the toast, the record read again.
  s.cache.refused('al-42');
  await settle();
  assert.equal(w.get().entity?.rev, 101);
  assert.deepEqual(sectionsOf(w.get().entity!).flatMap((x) => x.cards.flatMap((c) => c.items.map((i) => i.name))), [], 'Replace leaves');
  w.stop();
});

test('two wants show requested once', async () => {
  const runs = new Runs();
  let posts = 0;
  let done!: () => void;
  const want = () => new Promise<void>((ok) => ((posts += 1), (done = ok)));
  const first = runs.once('want', want);
  const second = runs.once('want', want);
  assert.equal(posts, 1, 'one request for two taps within 50 ms');
  done();
  assert.equal(await first, true);
  assert.equal(await second, false, 'the second tap did nothing: no second toast');
  const s = server();
  s.now.set('al-7', (p) => entity('al-7', p, { rev: 10, is: 'not_had', words: 'Not in your library', actions: [act('requested', 'play', { group: 'want' })] }));
  const w = shown(s.cache, 'al-7', { from: 'search' });
  await settle();
  assert.deepEqual(playRow(w.get().entity!).map((a) => a.name), ['requested'], 'Requested once, Want gone');
  w.stop();
});

test('two taps delete once', async () => {
  const runs = new Runs();
  let deletes = 0;
  const del = async () => void (deletes += 1);
  await Promise.all([runs.once('delete:MUSIC', del), runs.once('delete:MUSIC', del)]);
  assert.equal(deletes, 1);
  await runs.once('delete:MUSIC', del);
  assert.equal(deletes, 2, 'once the first is done, a tap is a tap again');
});

test('an answer and a change out of order keep the newest', async () => {
  const s = server();
  s.now.set('al-42', MUSIC_ONLY(139));
  const w = shown(s.cache, 'al-42', { from: 'library' });
  await settle();
  s.now.set('al-42', MUSIC_ONLY(140));
  s.cache.told({ rev: 140, ids: [{ id: 'al-42', op: 'changed' }] });
  await settle();
  assert.equal(w.get().entity?.rev, 140);
  // The move's answer comes in GF's shape `{ result, undo, rev: 141 }`: read again until a read at or after 141 lands.
  s.now.set('al-42', MUSIC_IN(141));
  s.cache.after('al-42', 141);
  await settle();
  assert.equal(w.get().entity?.rev, 141);
  assert.equal(w.get().current, true);
  w.stop();
});

test('the stream down or the tab hidden: the change log tells what changed, a reset reads what is on screen and drops the rest', async () => {
  const s = server();
  s.now.set('al-42', MUSIC_IN(100));
  s.now.set('al-9', (p) => entity('al-9', p, { rev: 90 }));
  const nine = shown(s.cache, 'al-9', { from: 'library' });
  await settle();
  nine.stop();
  const w = shown(s.cache, 'al-42', { from: 'library' });
  await settle();
  s.now.set('al-42', MUSIC_ONLY(150));
  // GET /api/changes?since=100&kinds=record: al-42 changed at 120 and 150.
  onChanged(s.cache, { rev: 150, ids: [{ id: 'al-42', op: 'changed' }] }, () => {});
  await settle();
  assert.equal(w.get().entity?.rev, 150);
  // Since before the floor: a reset.
  s.now.set('al-42', MUSIC_ONLY(200));
  onChanged(s.cache, { rev: 200, reset: 'record' }, () => {});
  await settle();
  assert.equal(w.get().entity?.rev, 200, 'every record on screen read again');
  assert.equal(s.cache.client.getQueryCache().findAll({ queryKey: ['record', 'al-9'] }).length, 0, 'the rest dropped');
  w.stop();
});

test('a burst says records stale: what is on screen is read, the rest marked', async () => {
  const s = server();
  s.now.set('al-42', MUSIC_ONLY(100));
  const w = shown(s.cache, 'al-42', { from: 'library' });
  await settle();
  s.now.set('al-42', MUSIC_ONLY(300));
  onChanged(s.cache, { rev: 300, stale: 'record' }, () => {});
  await settle();
  assert.equal(w.get().entity?.rev, 300);
  w.stop();
});

test('the one loading rule', () => {
  const e = entity('al-42', { from: 'library' }, { rev: 1 });
  const base: Shown = { id: 'al-42', entity: null, current: false, fetching: true, gone: null, error: null };
  assert.equal(look({ ...base, entity: e, current: true }, 0), 'entity', 'current: shown at once');
  assert.equal(look({ ...base, entity: e }, KEEP_MS - 1), 'kept', 'stale: what was shown kept up to 300 ms');
  assert.equal(look({ ...base, entity: e }, KEEP_MS), 'loader', 'then the standard loader');
  assert.equal(look(base, KEEP_MS - 1), 'none', 'nothing cached: no loader before 300 ms');
  assert.equal(look(base, KEEP_MS), 'loader');
  assert.equal(look({ ...base, fetching: false, error: new Error('x') }, 0), 'error', 'an error: the existing notice with Try again');
});

test("a switch reads again what follows it", async () => {
  const s = server();
  const ownA = (p: Place) => !(p.without ?? []).includes('cratarr/own-a');
  const music = (p: Place) => !(p.without ?? []).includes('cratarr/music');
  s.now.set('al-42', (p) => {
    const v = [music(p) ? copy('MUSIC', 'MUSIC', 'MP3 320', true) : null, ownA(p) ? copy('OWN-A', 'Your music', 'FLAC', !music(p)) : null].filter((x): x is Version => !!x);
    return entity('al-42', p, { rev: 100, is: v.length ? 'had' : 'not_had', words: v.length ? 'In your library' : 'Not in your library', actions: v.length ? [act('play', 'play')] : [], versions: v.length > 1 ? v : [] });
  });
  let w = shown(s.cache, 'al-42', { from: 'artist', without: [] });
  await settle();
  w.stop();
  w = shown(s.cache, 'al-42', { from: 'artist', without: ['cratarr/music'] });
  await settle();
  assert.equal(w.get().entity?.state.words, 'In your library', 'after MUSIC: the Head still says it');
  w.stop();
  w = shown(s.cache, 'al-42', { from: 'artist', without: ['cratarr/music', 'cratarr/own-a'] });
  await settle();
  assert.equal(w.get().entity?.state.words, 'Not in your library', 'after OWN-A: as the artist page lists it');
  assert.deepEqual(playRow(w.get().entity!), [], 'no Play row');
  w.stop();
});
