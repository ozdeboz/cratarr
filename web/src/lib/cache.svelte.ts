// The one page cache's binding (DOMAIN-CORE §13; lib/recordCache holds its logic, on @tanstack/query-core): one cache for the
// page, asking the server's GET /api/records/{id}, kept current by the live channel's `changed` (and the change log's, which
// lib/changes tells the same way), and a record as a component reads it, a rune that follows its place.
import { ApiError, getRecord } from './api';
import { on } from './events.svelte';
import { loadMe } from './me.svelte';
import { KEEP_MS, look, onChanged, RecordCache, type Changed, type Place, type Shown } from './recordCache';

export type { Place, RecordEntity, Shown } from './recordCache';

/** The page's one record cache. */
export const records = new RecordCache(getRecord, (e) => (e instanceof ApiError && e.status === 404 ? e.message : null));

on<Changed>('changed', (c) => onChanged(records, c, () => void loadMe()));

/** A record as a sheet or a page shows it (LV5–LV7), read through the cache and kept current while it shows: `shown`, and
 *  `look`, the one loading rule's verdict: from the moment nothing current shows, what was shown is kept 300 ms, then the
 *  loader. */
export function recordOf(get: () => { id: string; place: Place }) {
  const s = $state<{ shown: Shown | null; since: number; now: number }>({ shown: null, since: Date.now(), now: Date.now() });
  $effect(() => {
    const { id, place } = get();
    let t: ReturnType<typeof setTimeout> | undefined;
    let waiting = false;
    const waitFrom = () => {
      s.since = s.now = Date.now();
      clearTimeout(t);
      t = setTimeout(() => (s.now = Date.now()), KEEP_MS);
    };
    waitFrom();
    const stop = records.watch(id, place, (x) => {
      if (!x.current && !waiting) waitFrom();
      waiting = !x.current;
      s.shown = x;
    });
    return () => {
      clearTimeout(t);
      stop();
    };
  });
  return {
    get shown() {
      return s.shown;
    },
    get look() {
      return s.shown ? look(s.shown, s.now - s.since) : 'none';
    },
  };
}
