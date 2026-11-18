// The Users panel and Settings (AL04.4; decision 0084: a room in GB per user, set in the Users panel; the disk floor in
// Settings): the permissions as the panel lists them, each user's room beside their other rows opening the value sheet every
// value has, and the floor as a number of GB. Each room is its account's (AL04.2): one's own line is read from /api/me, the
// panel with everyone's is the administrators'. These read the source where a page needs a whole app to run.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
(globalThis as Record<string, unknown>).$effect = () => {};
const { partsFor } = await import('../src/lib/settingsSections.svelte.ts');
const { floorRow } = await import('../src/lib/room.ts');

const source = (file: string) => readFileSync(new URL(`../src/${file}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test('the permissions hold Request albums and no Approve requests automatically (AL04.4)', () => {
  const page = source('lib/settings/UsersPart.svelte');
  const list = page.slice(page.indexOf('const PERMISSIONS'), page.indexOf('];', page.indexOf('const PERMISSIONS')));
  const labels = [...list.matchAll(/label: '([^']+)'/g)].map((m) => m[1]);
  assert.deepEqual(labels, ['Download in apps', 'Change playlists in apps', 'Share playlists', 'Request albums', 'Answer requests']);
  assert.ok(!page.includes('Approve requests automatically') && !page.includes('auto_approve'));
});

test("each user's room is a row beside their others, its line the server's, opening the value sheet for its GB (AL04.4)", () => {
  const page = source('lib/settings/UsersPart.svelte');
  assert.ok(page.includes(`{#if !u.admin && u.room}<Item name="Room" value={lineOfRoom(u.room)} onclick={() => (sizing = u)} />{/if}`), 'none for an administrator');
  // Beside the existing rows of both kinds of account, a directory's and Cratarr's own.
  assert.equal(page.split('{@render roomRow(u)}').length - 1, 2);
  assert.ok(page.includes("import ValueSheet from './ValueSheet.svelte';"), 'the value sheet every value has');
  assert.ok(
    page.includes(
      '<ValueSheet title={`Room of ${u.name}`} type="number" value={gbOfRoom(sizing.room)} suffix="GB" min={0} allowEmpty={false} saveLabel="Save" onsave={(v) => sizeRoom(u, Number(v))} onclose={() => (sizing = null)} />',
    ),
  );
  // Read again only once the server took it: refused, the room stays as it was and the sheet stays (the refusal thrown on).
  const sized = page.slice(page.indexOf('async function sizeRoom'), page.indexOf('let passwording'));
  assert.match(sized, /try \{\n\s+await setUserRoom\(u\.name, gb\);\n\s+await load\(\);\n\s+\} catch \(e\) \{\n\s+toast\.fail\(e, 'Could not set the room'\);\n\s+throw e;/);
});

test('Settings has the floor as a number of GB, in its own row and sheet (AL04.4)', () => {
  const page = source('lib/settings/LibrariesPart.svelte');
  assert.ok(page.includes('<Group title="Disk space">'));
  assert.ok(page.includes('<Item name="Floor" more={FLOOR_MORE} value={floorShown.value} line={floorShown.line} tone={floorShown.tone} onclick={() => (flooring = true)} />'));
  assert.ok(page.includes('const floorShown = $derived(floorRow(floor.value, reach.away));'));
  assert.match(page, /<ValueSheet\n\s+title="Floor"\n\s+value=\{String\(floor\.value\.gb\)\}\n\s+type="number"\n\s+suffix="GB"\n\s+min=\{0\}\n\s+allowEmpty=\{false\}\n\s+onsave=\{\(v\) => saveFloor\(Number\(v\)\)\}/);
  // The server's 200 GB unless set, the free space where users' music lands, warned while short.
  assert.deepEqual(floorRow({ gb: 200, free: 150e9, short: true }), { value: '200 GB', line: '150 GB free', tone: 'warn' });
  assert.deepEqual(floorRow({ gb: 200, free: 1_234_567e6, short: false }), { value: '200 GB', line: '1.2 TB free', tone: null });
  assert.deepEqual(floorRow({ gb: 0, free: null, short: false }), { value: '0 GB', line: null, tone: null });
  assert.equal(floorRow({ gb: 200, free: 209.9e9, short: true })?.line, '209 GB free', 'never rounded up past the floor');
});

test("each room is its account's: one's own line from /api/me, everyone's in the administrators' panel only (AL04.2)", () => {
  const ids = (admin: boolean) => partsFor(admin, false, false).map((p: { id: string }) => p.id);
  assert.ok(ids(true).includes('users') && !ids(false).includes('users'), "the panel that lists every user's room");
  const panel = source('lib/RequestsPanel.svelte');
  assert.ok(panel.includes('const roomSaid = $derived(lineOfRoom(me.room, reach.away));'), 'one\'s own, as /api/me says it');
  assert.ok(panel.includes('{#if roomSaid}<Group><Item name="Room" value={roomSaid} /></Group>{/if}'));
  assert.ok(panel.includes("if (list === 'mine') void readRoom();"), 'read again where it shows');
  for (const file of ['lib/RequestsPanel.svelte', 'lib/me.svelte.ts', 'lib/requestState.ts']) assert.ok(!source(file).includes('getUsers'), `${file} reads no one else's`);
});
