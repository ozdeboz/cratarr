// What changed while this page was away (decision 0087): Cratarr's change log read from the last change seen, on every
// reconnect, once what waited in the outbox is sent, and every minute while the page is looked at. What changed is told
// as the live channel's `changed` (lib/events), `{ area, ids }` (ids null: all of the area), so the views that read a
// list again on the server's word read it again here too. Only where the server keeps a log (its `changes`).
import { accountKey, whenAccount } from './account';
import { getChanges } from './api';
import { can } from './capabilities.svelte';
import { readChanges, type Refresh } from './changeLog';
import { indexStands, indexSync } from './deviceSync.svelte';
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

const REV = 'changes-rev';
/** The last change seen, per account (lib/account's key; the owner, 2026-11-02: with one for the device, an account
 *  signing in after another read only what changed after the other's). One new here starts from nothing, as a fresh
 *  device does. */
let revs: Record<string, number> = {};
/** The one rev kept before 0.13: the account known first's. */
let before: number | null = null;
const known =
  typeof window === 'undefined'
    ? Promise.resolve()
    : readKept<unknown>(REV).then((v) => {
        if (typeof v === 'number') before = v;
        else if (v && typeof v === 'object') revs = { ...(v as Record<string, number>) };
      });
let reading = false;

/** Reads the log from the last change seen and tells what changed. The first time (no change seen yet) it only takes
 *  where the log stands: the page has just read everything. */
export async function readChangesNow() {
  await known;
  const account = accountKey();
  if (reading || account === null || reach.away || can('changes') !== true) return;
  reading = true;
  try {
    if (before !== null) {
      revs[account] ??= before;
      before = null;
    }
    const first = revs[account] === undefined;
    const refresh: Refresh = new Map();
    for (let page = 0; page < PAGES; page++) {
      const answer = await getChanges(revs[account] ?? 0, KINDS, indexSync.rev);
      if (page === 0) void indexStands(answer.index);
      const r = readChanges(KINDS, answer, refresh);
      revs[account] = r.rev;
      if (!r.more) break;
    }
    await keep(REV, revs);
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
  whenAccount.add(() => void readChangesNow());
  setInterval(() => !document.hidden && void readChangesNow(), EVERY_MS);
  void readChangesNow();
}
