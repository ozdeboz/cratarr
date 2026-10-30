// What changed while this page was away (decision 0087): Cratarr's change log read from the last change seen, on every
// reconnect, once what waited in the outbox is sent, and every minute while the page is looked at. What changed is told
// as the live channel's `changed` (lib/events), `{ area, ids }` (ids null: all of the area), so the views that read a
// list again on the server's word read it again here too. Only where the server keeps a log (its `changes`).
import { getChanges } from './api';
import { can } from './capabilities.svelte';
import { readChanges, type Refresh } from './changeLog';
import { emit } from './events.svelte';
import { keep, readKept } from './keptStore';
import { whenSent } from './later.svelte';
import { reach, whenReached } from './reach.svelte';

/** The log's kinds the page shows, and the `changed` area each is told as. */
const AREAS: Record<string, string> = { playlist: 'playlists', favourite: 'favourites', rating: 'ratings', excluded: 'excluded' };
const KINDS = Object.keys(AREAS);
const EVERY_MS = 60_000;
/** Pages of the log read in one go at most (a long time away: the rest next time). */
const PAGES = 20;

let rev: number | null = null;
const known = typeof window === 'undefined' ? Promise.resolve() : readKept<number>('changes-rev').then((v) => void (typeof v === 'number' && (rev = v)));
let reading = false;

/** Reads the log from the last change seen and tells what changed. The first time (no change seen yet) it only takes
 *  where the log stands: the page has just read everything. */
export async function readChangesNow() {
  await known;
  if (reading || reach.away || can('changes') !== true) return;
  reading = true;
  try {
    const first = rev === null;
    const refresh: Refresh = new Map();
    for (let page = 0; page < PAGES; page++) {
      const r = readChanges(KINDS, await getChanges(rev ?? 0, KINDS), refresh);
      rev = r.rev;
      if (!r.more) break;
    }
    await keep('changes-rev', rev);
    if (first) return;
    for (const [kind, ids] of refresh) emit('changed', { area: AREAS[kind], ids: ids === 'all' ? null : [...ids] });
  } catch {
    /* next time */
  } finally {
    reading = false;
  }
}

if (typeof window !== 'undefined') {
  whenReached.add(() => void readChangesNow());
  whenSent.add(() => void readChangesNow());
  setInterval(() => !document.hidden && void readChangesNow(), EVERY_MS);
  void readChangesNow();
}
