// A request's sheet tells its whole story (RQ01; the owner, preview 1's F4: its sheet held only its actions, "a lot more could be
// displayed"): where it stands, as its row says it; each fetcher's part (Lidarr's step, Soulseek's item whole: its sources by
// user, the one fetching and how far, held until when and why, stuck and why, the users that failed it); what was set aside, file
// by file and why; when it was asked and delivered, and where its asker's copy is. Its asker reads all of it, Soulseek's user
// names included; only administrators act (a source picked and fetched, Search now, Hand over again). A state's words are the
// shared rules' (W5's `standing`, the Soulseek list's `itemLine` and `folderLine`), never worked out again here; Lidarr's own
// words are the administrators' (W6). In the sheet's own pieces (lib/RequestsPanel: Group, Item, ChoiceList, MenuEntry). Pure.
import type { RequestStory, StoryLidarr, StorySoulseek, StorySource } from './api';
import { handOverAgain, mayTakeBack, standing, standingTone, WAITS_FOR } from './requestState';
import { folderLine, itemLine, searchable, tickable, underWay, type SoulseekFolder, type SoulseekItem } from './soulseekRules';
import { when } from './settings/when';

/** A row of the sheet (lib/settings/Item): its name, line, value, tone, lead dot and fill (0 to 1). */
export type StoryItem = { name: string; line?: string | null; value?: string | null; tone?: 'warn' | 'bad' | null; lead?: 'warn' | 'bad' | null; progress?: number | null; wrap?: boolean };
/** The sources to pick one from (lib/ChoiceList): an administrator's, where Found. */
export type StoryChoice = { choice: { label: string; options: { value: string; label: string; line: string }[]; chosen: string } };
export type StoryRow = StoryItem | StoryChoice;
/** Its groups, in this order, each only where it has something. */
export type StoryGroup = { title: 'Lidarr' | 'Soulseek' | 'Set aside' | 'Asked'; rows: StoryRow[] };
/** Its menu's entries (lib/MenuEntry), in this order. */
export type StoryAction = 'fetch' | 'searchNow' | 'handOverAgain' | 'view' | 'takeBack';
/** The sheet: its head (the row's own line, and its tone), its groups and its actions. */
export type StorySheet = { head: string; tone: 'bad' | null; groups: StoryGroup[]; actions: StoryAction[] };

/** Lidarr's steps as the sheet says them; a wait in the words the row says it in (PB4: what is true, never "Not handed over"
 *  for a Want no fetcher was to have yet). Red only where something failed: stuck, or not handed over. */
const LIDARR: Record<StoryLidarr['step'], string> = {
  looking: 'Looking for it',
  grabbed: 'Grabbed',
  importing: 'Importing',
  imported: 'Imported',
  stuck: 'Stuck',
  not_handed: 'Not handed over',
  waits_for_room: WAITS_FOR.room,
  waits_for_space: WAITS_FOR.space,
  waits_for_release: WAITS_FOR.release,
  stopped: 'Told to stop: it arrived',
};

/** Lidarr's row: its step, how far while grabbed (with its fill), the release day it waits for, and for an administrator
 *  Lidarr's own words. */
function lidarrRow(l: StoryLidarr, admin: boolean): StoryItem {
  const done = l.step === 'grabbed' && l.done != null ? l.done : null;
  const said = admin && l.said ? l.said : null;
  const day = l.step === 'waits_for_release' && l.day ? l.day : null;
  const line = [done !== null ? `${Math.round(done * 100)}%` : null, day, said].filter(Boolean).join(' · ') || null;
  return { name: LIDARR[l.step], line, progress: done, tone: l.step === 'stuck' || l.step === 'not_handed' ? 'bad' : null, wrap: !!said };
}

/** A source as the Soulseek list's folder (lib/soulseekRules). */
const folderOf = (x: StorySource): SoulseekFolder => ({
  user: x.user,
  dir: x.folder,
  format: x.format,
  lossless: x.lossless,
  free_slot: x.free_slot,
  queue: x.queue,
  speed: x.speed,
  extra: x.extra,
  unchecked: x.unchecked ?? null,
});
/** A source's line: the list's folderLine without its peer, who names its row. */
const sourceLine = (x: StorySource) => folderLine({ ...folderOf(x), user: '' });

/** Soulseek's part as the list's rules read an item, its record unnamed: so its line is itemLine's words without the artist and
 *  year, and what can be ticked, searched now or is under way is the list's rule. Held: why and until when; stuck: why; waiting:
 *  its next search. A folder set aside is only counted here, never its path. */
function itemOf(p: StorySoulseek): SoulseekItem {
  return {
    id: p.id,
    record: { record: 0, artist: '', album: '', year: null, request: 0 },
    state: p.state,
    folders: p.sources.map(folderOf),
    chosen: p.chosen ?? null,
    failed: p.failed,
    misses: p.misses,
    due: p.held?.until ?? p.next_search ?? 0,
    why: p.held?.why ?? p.stuck ?? null,
    percent: p.percent ?? 0,
    set_aside: p.set_aside.map((a) => ({ user: a.user, path: '', why: {} })),
  };
}

/** Soulseek's rows: its line; its sources, each named by its peer (the one fetching with its fill), or for an administrator where
 *  Found and slskd is on, the choice of them; the users that failed it. */
function soulseekRows(p: StorySoulseek, admin: boolean, now: number): StoryRow[] {
  const i = itemOf(p);
  const rows: StoryRow[] = [{ name: itemLine(i, now) }];
  if (admin && !p.off && tickable(i))
    rows.push({ choice: { label: 'Source', options: p.sources.map((x, n) => ({ value: String(n), label: x.user, line: sourceLine(x) })), chosen: String(p.chosen ?? 0) } });
  else {
    const fetching = p.state === 'fetching' ? p.chosen : undefined;
    p.sources.forEach((x, n) => rows.push({ name: x.user, line: sourceLine(x), progress: n === fetching ? (p.percent ?? 0) / 100 : null }));
  }
  if (p.failed.length) rows.push({ name: 'Failed', line: p.failed.join(' · ') });
  return rows;
}

/** What was set aside: each folder named by its peer, each file with why. */
const setAside = (p: StorySoulseek): StoryItem[] =>
  p.set_aside.map((a) => ({ name: a.user, line: a.files.map((f) => `${f.file}: ${f.why}`).join(' · '), lead: 'warn', tone: 'warn', wrap: true }));

/** When it was asked and delivered (as the page says a time), and the library its asker's copy is in. */
const asked = (s: RequestStory, said: (t: number) => string): StoryItem[] => [
  { name: 'Asked', value: said(s.asked_at) },
  ...(s.delivered_at ? [{ name: 'Delivered', value: said(s.delivered_at) }] : []),
  ...(s.where ? [{ name: 'Where', value: s.where.name }] : []),
];

/** A request's sheet from its story, for its asker or an administrator (`admin`) at `now` (seconds); `said` says when. */
export function requestStory(s: RequestStory, admin: boolean, now: number, said: (t: number) => string = when): StorySheet {
  const p = s.soulseek ?? null;
  const groups: StoryGroup[] = [];
  if (s.lidarr) groups.push({ title: 'Lidarr', rows: [lidarrRow(s.lidarr, admin)] });
  if (p) groups.push({ title: 'Soulseek', rows: soulseekRows(p, admin, now) });
  if (p?.set_aside.length) groups.push({ title: 'Set aside', rows: setAside(p) });
  groups.push({ title: 'Asked', rows: asked(s, said) });
  // An administrator's calls on Soulseek, none while slskd is switched off: Fetch where Found; Search now where it waits, is
  // stuck, or Found (searched afresh, as the server's search again takes it), never while it moves.
  const i = p && !p.off ? itemOf(p) : null;
  const actions: StoryAction[] = [];
  if (admin && i && tickable(i)) actions.push('fetch');
  if (admin && i && (searchable(i) || i.state === 'found')) actions.push('searchNow');
  if (handOverAgain(s, admin)) actions.push('handOverAgain');
  if (s.opens) actions.push('view');
  // Whoever reads a story is its asker or an administrator: Take back by the one rule (RF18).
  if (mayTakeBack(s, s.user, admin)) actions.push('takeBack');
  return { head: standing(s, said), tone: standingTone(s), groups, actions };
}

/** How long until the sheet reads its story again (RQ01.8 (d)): every 3 s while Soulseek's part is under way by the list's own
 *  rule (searching, to be fetched, fetching, checking, and not held), else every 30 s; with slskd switched off nothing moves. */
export const storyAgainIn = (s: Pick<RequestStory, 'soulseek'>, now: number): number =>
  s.soulseek && !s.soulseek.off && underWay({ items: [itemOf(s.soulseek)] }, now) ? 3000 : 30_000;

/** What Fetch sends for the source picked (none picked: the one chosen, else the first): the item's id, the source's place, and
 *  its user and folder's name, so a tick read before a new search changed the folders is refused, never fetched from another
 *  (RQ01.8 (b)). */
export function tickOf(p: StorySoulseek, picked: number | null): { id: string; folder: number; user: string; folder_name: string } | null {
  const n = picked ?? p.chosen ?? 0;
  const x = p.sources[n];
  return x ? { id: p.id, folder: n, user: x.user, folder_name: x.folder } : null;
}
