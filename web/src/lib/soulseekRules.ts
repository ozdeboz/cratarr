// Soulseek in Cratarr (decision 0099 and its amendment), the page's rules, pure: what a record's row says where it
// stands, which can be ticked, how the list groups, and what Fetch sends. The server searches the requests approved in
// Cratarr by their release, finds folders that hold every track, and fetches only what is ticked.

export type SoulseekState = 'waiting' | 'searching' | 'found' | 'approved' | 'fetching' | 'checking' | 'placed' | 'stuck';
export type SoulseekFolder = { user: string; dir: string; format: string; lossless: boolean; free_slot: boolean; queue: number; speed: number; extra: number };
export type SoulseekItem = {
  id: string;
  record: { artist: string; album: string; release_group: string; year?: number | null };
  /** The approved Request it fetches (every one is one: decision 0099's amendment). */
  request: number;
  state: SoulseekState;
  folders: SoulseekFolder[];
  /** The folder to be fetched (its place in `folders`). */
  chosen?: number | null;
  /** Users who failed it: never asked again. */
  failed: string[];
  /** Searches in a row that found nothing, and when it is searched again (epoch s; 0: due). */
  misses: number;
  due: number;
  why?: string | null;
  /** How far its fetch is, 0 to 100. */
  percent: number;
  /** Where it was put, in its Incoming library. */
  placed?: string | null;
  /** Folders set aside, each file's why. */
  set_aside: { user: string; path: string; why: Record<string, string> }[];
};
export type SoulseekView = { items: SoulseekItem[]; library?: string | null };

/** A folder as a line: its format, how soon (a free slot, else its queue), its extra files, whose. */
export function folderLine(f: SoulseekFolder): string {
  const mb = f.speed > 0 ? `${(f.speed / 1_000_000).toFixed(1)} MB/s` : null;
  return [f.format.toUpperCase(), f.free_slot ? 'free slot' : `queue ${f.queue}`, mb, f.extra ? `${f.extra} extra file${f.extra === 1 ? '' : 's'}` : null, f.user]
    .filter(Boolean)
    .join(' · ');
}

/** When it is searched again, from `now` (epoch s): "again in 4 hours", "again tomorrow"; none when due. */
export function againIn(due: number, now: number): string | null {
  if (!due || due <= now) return null;
  const h = Math.round((due - now) / 3600);
  if (h < 1) return 'again within the hour';
  if (h < 20) return `again in ${h} hour${h === 1 ? '' : 's'}`;
  const d = Math.round(h / 24);
  return d <= 1 ? 'again tomorrow' : `again in ${d} days`;
}

/** Its row's line: the artist and year, then where it stands. */
export function itemLine(i: SoulseekItem, now = Math.floor(Date.now() / 1000)): string {
  const head = [i.record.artist, i.record.year ? String(i.record.year) : null];
  const best = i.folders[i.chosen ?? 0];
  const where = ((): (string | null)[] => {
    switch (i.state) {
      case 'waiting':
        return i.misses ? ['Not found', i.misses > 1 ? `${i.misses} times` : null, againIn(i.due, now)] : ['Waiting to be searched'];
      case 'searching':
        return ['Looking for it'];
      case 'found':
        return [best ? folderLine(best) : 'Found'];
      case 'approved':
        return ['To be fetched'];
      case 'fetching':
        return [`Fetching · ${Math.round(i.percent)}%`];
      case 'checking':
        return ['Checking its files'];
      case 'placed':
        return ['In Incoming'];
      case 'stuck':
        return [i.why || 'Stuck'];
    }
  })();
  const aside = i.set_aside.length ? `${i.set_aside.length} folder${i.set_aside.length === 1 ? '' : 's'} set aside` : null;
  return [...head, ...where, aside].filter(Boolean).join(' · ');
}

/** Only a found record with a folder can be ticked; nothing is fetched without a tick. */
export const tickable = (i: SoulseekItem) => i.state === 'found' && i.folders.length > 0;
/** Search now: one waiting for its next search, or stuck. */
export const searchable = (i: SoulseekItem) => i.state === 'waiting' || i.state === 'stuck';
/** Its row's tone: stuck said so; not found yet, a warning. */
export const itemTone = (i: SoulseekItem): 'bad' | 'warn' | null => (i.state === 'stuck' ? 'bad' : i.state === 'waiting' && i.misses ? 'warn' : null);

/** The list's parts, in this order: found (to tick), on its way, not found yet, stuck, placed. */
export const GROUPS: { title: string; states: SoulseekState[] }[] = [
  { title: 'Found', states: ['found'] },
  { title: 'On its way', states: ['approved', 'fetching', 'checking', 'searching'] },
  { title: 'Not found yet', states: ['waiting'] },
  { title: 'Stuck', states: ['stuck'] },
  { title: 'In Incoming', states: ['placed'] },
];

/** What Fetch sends: the ticked found records, each with the folder picked for it (else the server's best). */
export function approvePayload(items: readonly SoulseekItem[], ticked: ReadonlySet<string>, picks: Readonly<Record<string, number>>): { id: string; folder?: number }[] {
  return items.filter((i) => ticked.has(i.id) && tickable(i)).map((i) => (i.id in picks ? { id: i.id, folder: picks[i.id] } : { id: i.id }));
}

/** Whether the list is asked again soon: a search or a fetch under way. */
export const underWay = (v: SoulseekView | null) => !!v?.items.some((i) => i.state === 'searching' || i.state === 'approved' || i.state === 'fetching' || i.state === 'checking');
