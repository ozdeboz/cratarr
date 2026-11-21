// RS01, the one record sheet, drawn from the record's one entity (SCENARIOS-DC1 RS1–RS8, RS12, RS14, RS23, CN8, CN13, TY1, RV8;
// DOMAIN-CORE §8): the page lays out what the server sends (its sections in order, its actions and links by section and group,
// the Play row as sent, the Head's words and the versions) and decides nothing of a record itself. Pure parts and source checks.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import type { ActionView } from '../src/lib/generated/ActionView';
import type { LinkView } from '../src/lib/generated/LinkView';
import type { Section } from '../src/lib/generated/Section';
import type { Version } from '../src/lib/generated/Version';

const { deviceItems, headChips, PLACES, playRow, sectionsOf, stateLine, versionLines } = await import('../src/lib/sheet/record.ts');
const { when } = await import('../src/lib/settings/when.ts');
type Entity = import('../src/lib/recordCache.ts').RecordEntity;

const SRC = new URL('../src/', import.meta.url);
const read = (f: string) => readFileSync(new URL(f, SRC), 'utf8').replace(/\r\n/g, '\n');
const sources = new Map<string, string>();
(function walk(dir: string) {
  for (const e of readdirSync(new URL(dir, SRC), { withFileTypes: true })) {
    const path = dir + e.name;
    if (e.isDirectory()) {
      if (path !== 'lib/generated') walk(path + '/');
    } else if (/\.(svelte|ts)$/.test(e.name)) sources.set(path, read(path));
  }
})('');
const shared = (f: string) => JSON.parse(readFileSync(new URL(`../../shared/${f}`, import.meta.url), 'utf8'));

function act(name: string, section: Section, more: Partial<ActionView> = {}): ActionView {
  return { name, title: name, icon: 'play', group: section === 'play' ? 'play' : 'library', section, method: 'POST', href: `/api/x/${name}`, ...more };
}
function link(name: string, group: LinkView['group'], href: string): LinkView {
  return { name, title: name, icon: 'user', group, section: 'go', href };
}
function entity(more: Partial<Entity> = {}): Entity {
  return {
    class: 'record',
    id: 'al-42',
    rev: 100,
    epoch: 'alice|User|Full|request|',
    properties: {
      type: { value: 'EP', by: [], versions: [] },
      type_why: '',
      title: { value: 'Night Ferry', by: [], versions: [] },
      artists: { value: [{ id: 'ar-1', name: 'Glass Harbour' }], by: [], versions: [] },
      credit: 'Glass Harbour',
      year: { value: 2019, by: [], versions: [] },
      labels: { value: [], by: [], versions: [] },
      cover: { value: null, by: [], versions: [] },
    },
    state: { is: 'had', words: 'In your library' },
    actions: [],
    sections: ['head'],
    entities: [],
    links: [],
    ...more,
  };
}

test('the Head draws the state as sent', () => {
  for (const s of shared('record-states.json').states as { is: Entity['state']['is']; words: string }[]) {
    const e = entity({ state: { is: s.is, words: s.words } });
    assert.equal(stateLine(e, when), s.words, `${s.is}: its words as sent`);
  }
  const at = Date.UTC(2025, 2, 3, 12) / 1000;
  const retired = entity({ state: { is: 'retired', words: 'Was in MORE until', at } });
  assert.equal(stateLine(retired, when), `Was in MORE until ${when(at)}`, 'its time said by the one when()');
  assert.deepEqual(headChips(entity()), ['Glass Harbour', 'EP', '2019'], 'artists by the one formatter, the type word, the year');
  assert.deepEqual(headChips(entity({ properties: { ...entity().properties, year: { value: null, by: [], versions: [] }, type: { value: null, by: [], versions: [] } } })), ['Glass Harbour', 'Record']);
  // No state word is written in the sheet: they come with the entity.
  for (const s of shared('record-states.json').states as { words: string }[]) {
    const words = s.words.split('{')[0];
    for (const [path, text] of sources) if (path.startsWith('lib/sheet/')) assert.ok(!text.includes(`'${words}`) && !text.includes(`>${words}`), `${path} writes "${words}"`);
  }
});

test('the Play row is drawn in the order sent: Want last, or first from Discover', () => {
  const row = [act('play', 'play'), act('shuffle', 'play'), act('want', 'play', { group: 'want' })];
  assert.deepEqual(playRow(entity({ actions: [act('move_to', 'library'), ...row] })).map((a) => a.name), ['play', 'shuffle', 'want']);
  const discover = [act('want', 'play', { group: 'want' }), act('play', 'play')];
  assert.deepEqual(playRow(entity({ actions: discover })).map((a) => a.name), ['want', 'play'], 'the page never reorders it');
  assert.deepEqual(playRow(entity({ actions: [act('requested', 'play', { group: 'want' })] })).map((a) => a.name), ['requested'], 'Requested in its place');
});

test('versions: copies and editions in one list, the line that plays marked and with no tap', () => {
  const v: Version[] = [
    { kind: 'copy', id: 'MUSIC', record: 'al-20', library: 'MUSIC', quality: 'MP3 320', plays: true },
    { kind: 'copy', id: 'IN', record: 'al-20', library: 'IN', quality: 'FLAC', plays: false, action: 'play_from' },
    { kind: 'edition', id: 'al-21', record: 'al-21', library: 'MORE', quality: 'FLAC', tracks: 12, plays: false, action: 'view' },
  ];
  const playFrom = act('play_from', 'library', { method: 'GET', href: '/api/albums/cratarr/al-20', fields: { from: 'IN' } });
  const lines = versionLines(entity({ id: 'al-20', entities: v, actions: [playFrom] }));
  assert.deepEqual(lines.map((l) => l.text), ['MUSIC · MP3 320 · plays', 'IN · FLAC', '12 tracks · MORE · FLAC']);
  assert.deepEqual(lines.map((l) => l.plays), [true, false, false], 'one line marked "plays"');
  assert.equal(lines[0].play, null);
  assert.equal(lines[0].open, null, 'the line that plays has no tap');
  assert.equal(lines[1].play, playFrom, "a tap on IN's plays from it: the server's Play from IN");
  assert.equal(lines[2].open, 'album:cratarr/al-21', 'a tap on the edition opens its record');
  assert.deepEqual(versionLines(entity({ entities: [v[1]] })).find((l) => l.play), undefined, 'no Play from sent: no tap');
  for (const l of lines) assert.doesNotMatch(l.text, /kb\/s/);
  assert.deepEqual(versionLines(entity()), [], 'a record of one version has no section');
});

test('the Library section draws what is sent, in its groups, each entry naming its copy as the server words it', () => {
  const sent = [
    act('play', 'play'),
    act('move_to', 'library', { title: 'Move to MORE from MUSIC', group: 'library', fields: { from: 'MUSIC', to: 'MORE' } }),
    act('save_to', 'library', { title: 'Save to Your music from MUSIC', group: 'library' }),
    act('delete_from', 'library', { title: 'Delete from MUSIC', group: 'remove', confirm: 'Delete from MUSIC?' }),
    act('play_from', 'library', { title: 'Play from IN', group: 'library' }),
    act('favourite', 'go', { title: 'Favourite', group: 'feel' }),
  ];
  const links = [link('view', 'go', 'album:cratarr/al-42'), link('web_search', 'know', 'https://duckduckgo.com/?q=x')];
  const e = entity({ actions: sent, links, sections: ['head', 'play', 'library', 'go'] });
  const parts = sectionsOf(e);
  assert.deepEqual(parts.map((p) => p.section), ['library', 'go'], "the entity's sections in order, after the Head and the Play row");
  const library = parts[0].cards;
  assert.deepEqual(library.map((c) => c.group), ['library', 'remove'], "the groups' order");
  assert.deepEqual(library.flatMap((c) => c.items.map((i) => i.title)), ['Move to MORE from MUSIC', 'Save to Your music from MUSIC', 'Play from IN', 'Delete from MUSIC'], 'the titles as sent');
  assert.equal(library[1].items[0].action?.confirm, 'Delete from MUSIC?', 'its confirm with it, asked first');
  assert.deepEqual(parts[1].cards.map((c) => [c.group, c.items.map((i) => i.name)]), [['go', ['view']], ['feel', ['favourite']], ['know', ['web_search']]], 'links beside actions, by group');
  assert.deepEqual(sectionsOf(entity({ sections: ['head', 'play'] })), [], 'an entry the server did not send is never drawn');
});

test('View is the server\'s link alone: none on the record\'s own page', () => {
  const page = entity({ links: [link('go_artist', 'go', 'artist:cratarr/ar-1')], sections: ['head', 'go'] });
  assert.ok(!sectionsOf(page).flatMap((p) => p.cards.flatMap((c) => c.items)).some((i) => i.name === 'view'));
  const sheet = read('lib/sheet/Record.svelte');
  assert.doesNotMatch(sheet, /viewName\(|ENTRY_WORDS\.view/, 'the sheet names no View of its own');
});

test('on this device: the server\'s actions, the device\'s own holding', () => {
  const sent = [act('download', 'device', { title: 'Download…', group: 'take' }), act('sync', 'device', { title: 'Sync to this device', group: 'take', method: 'GET' })];
  const e = entity({ actions: sent, sections: ['head', 'device'] });
  assert.deepEqual(deviceItems(e, { inApp: false, keeps: false }), [], 'never the section in a browser');
  assert.deepEqual(deviceItems(e, { inApp: true, keeps: true }).map((i) => i.name), ['remove_from_device'], 'kept here: Remove from this device');
  assert.deepEqual(deviceItems(e, { inApp: true, keeps: false }).map((i) => i.name), ['sync', 'download'], 'not kept: Sync and Download…, as sent');
  assert.deepEqual(deviceItems(entity(), { inApp: true, keeps: true }), [], 'the server sent none (carol): no section, kept or not');
});

test('every record opener opens the one sheet with its place', () => {
  const words: string[] = shared('when.json').places.map((p: { from: string }) => p.from);
  assert.deepEqual([...PLACES].sort(), [...new Set([...words, 'label'])].sort(), "the page's places are the when-table's words");
  let mounts = 0;
  for (const [path, text] of sources) {
    // The one sheet, mounted as every kind's sheet is named (AlbumMenu, TrackMenu…): `import RecordMenu from './sheet/Record.svelte'`.
    for (const m of text.matchAll(/<RecordMenu\b((?:=>|[^>])*?)\/>/g)) {
      mounts++;
      const from = m[1].match(/\bfrom=(?:"([a-z]+)"|\{(\w+)\})/);
      assert.ok(from, `${path}: a record opener without its place`);
      if (from[1]) assert.ok(PLACES.includes(from[1]), `${path}: "${from[1]}" is no place`);
      assert.match(m[1], /\bid=/, `${path}: a record by its id`);
    }
    // The sheet other servers' albums and this device's music keep: a Cratarr record goes to the one sheet first.
    if (/<AlbumMenu\b/.test(text)) assert.ok(/\{[#:](?:else )?if [^}\n]*\bopensRecord\(/.test(text), `${path}: a Cratarr record opened on the old sheet`);
  }
  assert.ok(mounts >= 10, `the openers moved: ${mounts}`);
  for (const f of ['lib/HealthPanel.svelte', 'lib/RecordProblemSheet.svelte', 'lib/HealthIncoming.svelte']) assert.doesNotMatch(read(f), /<AlbumMenu\b/, `${f}: only Cratarr's records`);
});

test('the sheet and the cache take the generated types; api.ts declares none of them', () => {
  for (const f of ['lib/sheet/record.ts', 'lib/sheet/Record.svelte', 'lib/recordCache.ts']) assert.match(read(f), /from '\.\.?\/(?:\.\.\/)?(?:lib\/)?generated\//, `${f} imports from generated/`);
  const api = read('lib/api.ts');
  for (const t of ['Entity', 'RecordProperties', 'ActionView', 'LinkView', 'Version', 'Shown', 'RecordRow', 'RecordCard', 'RecordChip', 'Section', 'FactView']) {
    assert.doesNotMatch(api, new RegExp(`export (?:type|interface) ${t}\\b`), `api.ts declares ${t}`);
  }
});

/** What decides a record's rights on the page; each left only where its own run removes it (named). */
const RIGHTS = /\.deletable\b|\.keepable\b|\.keep_to\b|\.replace_into\b|\bmayEdit\b|\boffersOf\b|\boffersIn\b|\bmay_request\b/g;
const WAITING: Record<string, string> = {
  'lib/copiesRule.ts': "offersOf: SeveralAlbums' offers until RS15's POST /api/records/several (then CN12 drops AlbumCopy's rights)",
  'lib/SeveralAlbums.svelte': "RS15: RS01 for several, on the server's POST /api/records/several",
  'Discover.svelte': "RV7: Discover's sheet, on the server's /api/listed",
  'lib/DiscoverEntries.svelte': "RV7: Discover's own, on the server's /api/listed",
  'lib/api.ts': "the answers' types: AlbumCopy's rights leave with CN12's server half, Discover's may_request with RV7",
};
test('the page reads no right for a record', () => {
  const found = new Map<string, number>();
  for (const [path, text] of sources) {
    // Settings' libraries (a library's own `keep_to`) and the visuals' patch graph (a node's `deletable`) hold no record.
    if (path.startsWith('lib/settings/') || path.startsWith('lib/visuals/')) continue;
    const n = [...text.matchAll(RIGHTS)].length;
    if (n) found.set(path, n);
  }
  for (const [path] of found) assert.ok(path in WAITING, `${path} decides a record's right on the page`);
  for (const path of Object.keys(WAITING)) assert.ok(found.has(path), `${path} reads no right any more: take it off the waiting list`);
  // The record's sheet, its cache and the sheet other servers' albums keep read no role or permission at all (CN8).
  for (const f of ['lib/sheet/Record.svelte', 'lib/sheet/record.ts', 'lib/recordCache.ts', 'lib/cache.svelte.ts', 'lib/AlbumMenu.svelte', 'lib/HealthIncoming.svelte']) {
    assert.doesNotMatch(read(f), /\bme\.admin\b|\bpermissions\.request\b|\b(?:copyPlaying|copyLines|playFromEntries|moveEntries|replaceEntries|deleteEntries|adds)\(/, `${f} reads a right`);
  }
});

test("no record entry's words or icon are written in the sheet: they are the server's table", () => {
  const sheet = read('lib/sheet/Record.svelte') + read('lib/sheet/record.ts');
  for (const e of shared('entries.json').entries as { name: string; title: string; icon: string }[]) {
    // A title with a blank by its words before it ("Move to "), any other whole.
    const words = e.title.split('{')[0];
    const quotes = e.title.includes('{') ? ["'", '"', '>'].map((o) => [o, '']) : [["'", "'"], ['"', '"'], ['>', '<']];
    for (const [open, end] of quotes) assert.ok(!sheet.includes(`${open}${words}${end}`), `the sheet writes "${words}" (${e.name})`);
  }
});

test('out of reach a record opens the sheet it opens today, until the server marks what works away (RS8, RS17: held)', () => {
  const sheet = read('lib/sheet/Record.svelte');
  assert.match(sheet, /export const opensRecord = \(source: string\) => source === OWN && !reach\.away;/);
});

test('times are said by one when(), with the year when not this year', () => {
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 14, 5).getTime() / 1000;
  assert.doesNotMatch(when(today), /\d{4}/, 'today: its time');
  assert.match(when(today), /14|2/);
  const thisYear = new Date(now.getFullYear(), now.getMonth() === 9 ? 8 : 9, 2, 12).getTime() / 1000;
  assert.doesNotMatch(when(thisYear), new RegExp(String(now.getFullYear())), 'this year: no year');
  const lastYear = new Date(now.getFullYear() - 1, 2, 3, 12).getTime() / 1000;
  assert.match(when(lastYear), new RegExp(String(now.getFullYear() - 1)), 'last year: with its year');
  // Every caller takes the year rule with it (L14): named here, so a new one is seen.
  const callers = [...sources].filter(([, t]) => /import \{[^}]*\b(?:when|lastSeen)\b[^}]*\} from '(?:\.\/|\.\.\/)*(?:lib\/)?(?:settings\/)?when'/.test(t)).map(([p]) => p).sort();
  assert.deepEqual(callers, [
    'Discover.svelte',
    'Health.svelte',
    'lib/PlayersSettings.svelte',
    'lib/QueuesSheet.svelte',
    'lib/RequestsPanel.svelte',
    'lib/recordNotHad.ts',
    'lib/requestStory.ts',
    'lib/settings/AccountPart.svelte',
    'lib/settings/ConnectionsPart.svelte',
    'lib/settings/DiagnosticsPart.svelte',
    'lib/settings/HomeAssistantPart.svelte',
    'lib/settings/KeyGroup.svelte',
    'lib/settings/McpPart.svelte',
    'lib/settings/SpotifyPart.svelte',
    'lib/settings/TastePart.svelte',
    'lib/settings/UsersPart.svelte',
    'lib/sheet/record.ts',
  ]);
});
