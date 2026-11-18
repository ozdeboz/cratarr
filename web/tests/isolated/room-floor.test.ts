// A room and the floor on the page against a stand-in for Cratarr (lib/floor, lib/me, lib/api, the real ones): a refused room
// or floor keeps the old one and says the server's words (AL04.4); one's room is read from /api/me alone, one's own (AL04.2);
// out of reach no room line and no floor show (AL04.5).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
g.window = globalThis;
const every = setInterval;
g.setInterval = (...args: Parameters<typeof setInterval>) => every(...args).unref();
g.matchMedia = () => ({ matches: false, addEventListener() {} });
g.screen = { width: 1000, height: 800 };
Object.defineProperty(globalThis, 'navigator', { value: { onLine: true, serviceWorker: { addEventListener() {} } }, configurable: true });

/** What Cratarr keeps, whether it can be reached, and what it was asked. */
const GB = 1e9;
let down = false;
let signedIn = 'ann';
let floorBytes = 200 * GB;
const rooms: Record<string, number> = { ann: 100 * GB };
let annUsed = 40 * GB;
const asked: string[] = [];
const json = (v: unknown, status = 200) => new Response(JSON.stringify(v), { status, headers: { 'content-type': 'application/json' } });
/** The server's rule for a size in GB (`put_floor`, `set_room`): a number from 0, else refused with its words. */
const sizeOf = (body: string | undefined) => {
  const gb = JSON.parse(body ?? '{}').gb;
  return typeof gb === 'number' && Number.isFinite(gb) && gb >= 0 ? Math.round(gb * GB) : null;
};
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const path = String(input);
  const method = init?.method ?? 'GET';
  asked.push(`${method} ${path}${init?.body ? ` ${init.body}` : ''}`);
  if (down) throw new TypeError('no network');
  if (path === '/api/me') {
    const room = signedIn === 'ann' ? { size: rooms.ann, used: annUsed, full: annUsed >= rooms.ann } : null;
    return json({ name: signedIn, admin: false, permissions: {}, room });
  }
  if (path === '/api/settings/floor') {
    if (method === 'PUT') {
      const b = sizeOf(init?.body as string);
      if (b === null) return json({ error: 'Give the floor in GB, from 0.' }, 400);
      floorBytes = b;
    }
    return json({ gb: floorBytes / GB, free: 150 * GB, short: 150 * GB < floorBytes });
  }
  const room = path.match(/^\/api\/users\/(\w+)\/room$/);
  if (room && method === 'PUT') {
    const b = sizeOf(init?.body as string);
    if (b === null) return json({ error: 'Give the room in GB, from 0.' }, 400);
    rooms[room[1]] = b;
    return new Response(null, { status: 204 });
  }
  return json({});
}) as typeof fetch;

const { setUserRoom } = await import('../../src/lib/api.ts');
const { floor, loadFloor, saveFloor } = await import('../../src/lib/floor.svelte.ts');
const { loadMe, me, readRoom } = await import('../../src/lib/me.svelte.ts');
const { reach } = await import('../../src/lib/reach.svelte.ts');
const { floorRow, lineOfRoom } = await import('../../src/lib/room.ts');

const refusal = async (p: Promise<unknown>) => {
  try {
    await p;
  } catch (e) {
    return (e as Error).message;
  }
  assert.fail('taken');
};

test("a refused floor or room keeps the old one and says the server's words (AL04.4)", async () => {
  await loadFloor();
  assert.deepEqual(floor.value, { gb: 200, free: 150 * GB, short: true });
  for (const typed of [-1, NaN]) {
    assert.equal(await refusal(saveFloor(typed)), 'Give the floor in GB, from 0.', String(typed));
    assert.equal(floor.value?.gb, 200, 'the floor as it was read');
    assert.equal(floor.error, 'Give the floor in GB, from 0.');
  }
  assert.ok(asked.includes('PUT /api/settings/floor {"gb":-1}') && asked.includes('PUT /api/settings/floor {"gb":null}'), 'the server is asked, and refuses');
  await saveFloor(120);
  assert.deepEqual([floor.value, floor.error], [{ gb: 120, free: 150 * GB, short: false }, null]);

  for (const typed of [-1, NaN]) assert.equal(await refusal(setUserRoom('ann', typed)), 'Give the room in GB, from 0.');
  assert.equal(rooms.ann, 100 * GB, "ann's room as it was");
  await setUserRoom('ann', 150);
  assert.ok(asked.includes('PUT /api/users/ann/room {"gb":150}'));
  assert.equal(rooms.ann, 150 * GB);
});

test("one's room is read from /api/me alone, and only for the account signed in (AL04.2)", async () => {
  await loadMe();
  assert.equal(lineOfRoom(me.room, reach.away), '40 of 150 GB');
  annUsed = 151 * GB;
  const from = asked.length;
  await readRoom();
  assert.equal(lineOfRoom(me.room, reach.away), '151 of 150 GB');
  assert.deepEqual(asked.slice(from), ['GET /api/me'], "nothing that holds anyone else's");
  // Another account signed in meanwhile: its answer is not taken for ann's.
  signedIn = 'bob';
  await readRoom();
  assert.equal(me.room?.used, 151 * GB);
  signedIn = 'ann';
});

test('out of reach: no room line and no floor (AL04.5)', async () => {
  down = true;
  await readRoom();
  await loadFloor();
  assert.ok(reach.away);
  assert.ok(me.room && floor.value, 'what was read is kept');
  assert.equal(lineOfRoom(me.room, reach.away), null);
  assert.equal(floorRow(floor.value, reach.away), null);
});
