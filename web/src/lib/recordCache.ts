// The one page cache (DOMAIN-CORE §13; SCENARIOS-DC1 LV2, LV4–LV7, LV9, RS18–RS22): every record the page shows is one entry of
// @tanstack/query-core's cache under ['record', id, epoch], the record's one entity as GET /api/records/{id} answers it. A card,
// a row, its page and its sheet that read one record under one epoch read one entry, so they change together and one request
// goes for it (LV5). No runes here, so the tests take it as it is; lib/cache.svelte.ts is its thin binding.
// - **The key's epoch is the server's** (LV4): what the answer was read under that is no row (the place's libraries, the viewer's
//   role and permissions). A place is asked by what its opener sends (`from`, the switch's `without`, the page's `opener`); the
//   epoch it answered under is kept for it, so the next record asked from there finds its key at once.
// - **Stale by the server's word:** `changed { rev, ids }` (the live channel), the change log's `record` kind (lib/changes), a
//   burst's "records stale", an account's rights changed. An entry read below the rev told is stale (LV2: revs only grow, so a
//   late answer never passes for current, LV6); what is on screen is read again at once, the rest when it shows.
// - **A record merged** answers as the one it went into (RV3, RS18): its sheet follows. **One gone** (RV4) answers 404 in the
//   server's words, and its entry goes.
import { QueryClient, QueryObserver, type QueryObserverResult } from '@tanstack/query-core';
import type { Entity } from './generated/Entity';
import type { RecordProperties } from './generated/RecordProperties';

export type RecordEntity = Entity<RecordProperties>;
/** Where a record is shown from (the when-table's `from`), with what its opener sends: the libraries switched off, and the
 *  record's own page's opener. */
export type Place = { from: string; without?: readonly string[]; opener?: string };
/** A record told changed (`changed { rev, ids }`, decision 0087's ops): changed, merged (`into` the one it went into), forgotten. */
export type ToldId = { id: string; op: string; into?: string | null };
/** What the live channel and the change log say of records: their ids at a rev, or all of them stale (a burst, LV12). */
export type Told = { rev?: number; ids?: ToldId[]; stale?: string };
/** A `changed` as the page hears it: records (Told), the log's reset of them, or an account's rights changed (CN10, M13). */
export type Changed = Told & { area?: string; reset?: string };
/** Whether a told change names this record (its id among the changed). */
export const namesRecord = (c: Told, id: string): boolean => !!c.ids?.some((x) => x.id === id);
/** The server asked for one record from one place. */
export type Ask = (id: string, place: Place) => Promise<RecordEntity>;

/** What a sheet or a page shows of a record now: its entity (null while none is known, or gone), whether it is current, and the
 *  server's words where it is gone. `id` is the record it is now (another after a merge). */
export type Shown = { id: string; entity: RecordEntity | null; current: boolean; fetching: boolean; gone: string | null; error: unknown };

/** A record's key in the cache: its id and the epoch it was read under. */
export const recordKey = (id: string, epoch: string) => ['record', id, epoch] as const;
type Key = ReturnType<typeof recordKey>;
/** A place as its opener asks it: the same words, the same place. */
export const placeKey = (p: Place) => [p.from, p.opener ?? '', [...(p.without ?? [])].sort().join(',')].join('|');
/** The key of a place not answered yet: its own until the server names the epoch. */
const unknown = (place: string) => `?${place}`;
const same = (a: readonly unknown[], b: readonly unknown[]) => a.length === b.length && a.every((x, i) => x === b[i]);

type Watcher = { asked: string; place: Place; key: Key; observer: QueryObserver<RecordEntity, Error, RecordEntity, RecordEntity, Key>; onchange: (s: Shown) => void };

export class RecordCache {
  readonly client: QueryClient;
  /** The epoch each place was answered under (LV4). */
  readonly #epochs = new Map<string, string>();
  /** Where an id went (RV3, RS18). */
  readonly #went = new Map<string, string>();
  /** Ids gone, with the server's words (RV4). */
  readonly #gone = new Map<string, string>();
  /** The newest rev each id was told changed at (LV2, LV6). */
  readonly #told = new Map<string, number>();
  /** The rev each id was read again for after a late answer: once per rev, never a loop. */
  readonly #again = new Map<string, number>();
  readonly #watchers = new Set<Watcher>();
  readonly ask: Ask;
  readonly goneOf: (e: unknown) => string | null;

  /** `goneOf`: the server's words where an answer says the record is gone (a 404), else null. */
  constructor(ask: Ask, goneOf: (e: unknown) => string | null) {
    this.ask = ask;
    this.goneOf = goneOf;
    // Kept current by what the server tells, not by time: fresh until told otherwise; a refusal is no reason to ask again.
    this.client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, gcTime: 5 * 60_000, retry: false, refetchOnWindowFocus: false, refetchOnReconnect: false } } });
  }

  /** The record an id is now: the one a merge took it into (RV3). */
  resolve(id: string): string {
    for (let i = 0; i < 20 && this.#went.has(id); i++) id = this.#went.get(id)!;
    return id;
  }

  #keyOf(asked: string, place: Place): Key {
    const pk = placeKey(place);
    return recordKey(this.resolve(asked), this.#epochs.get(pk) ?? unknown(pk));
  }

  #options(w: Pick<Watcher, 'key' | 'place'>) {
    return { queryKey: w.key, queryFn: () => this.#read(w.key, w.place), enabled: !this.#gone.has(w.key[1]) };
  }

  /** Shows record `id` from `place`: `onchange` with what it shows, now and on every change, until the returned function. Read
   *  where nothing current is cached. */
  watch(id: string, place: Place, onchange: (s: Shown) => void): () => void {
    const key = this.#keyOf(id, place);
    const w = { asked: id, place, key, onchange } as Watcher;
    w.observer = new QueryObserver(this.client, this.#options(w));
    this.#watchers.add(w);
    const stop = w.observer.subscribe((r) => {
      this.#readAgainIfLate(w, r);
      onchange(this.#shown(w, r));
    });
    onchange(this.#shown(w, w.observer.getCurrentResult()));
    return () => {
      stop();
      this.#watchers.delete(w);
      const id = this.resolve(w.asked);
      if (this.#gone.has(id)) this.client.removeQueries({ queryKey: ['record', id] });
    };
  }

  #shown(w: Watcher, r: QueryObserverResult<RecordEntity, Error>): Shown {
    const id = this.resolve(w.asked);
    const gone = this.#gone.get(id) ?? null;
    const e = gone ? null : (r.data ?? null);
    const current = !!e && !r.isStale && e.rev >= (this.#told.get(e.id) ?? 0);
    return { id, entity: e, current, fetching: r.isFetching, gone, error: r.error };
  }

  /** An answer read below a rev told since (LV6): read once more for that rev, while it is on screen. */
  #readAgainIfLate(w: Watcher, r: QueryObserverResult<RecordEntity, Error>) {
    const e = r.data;
    // Marked stale already: its own read is on the way.
    if (!e || r.isFetching || r.isStale || w.observer.getCurrentQuery().state.fetchStatus !== 'idle') return;
    const told = this.#told.get(e.id) ?? 0;
    if (e.rev >= told || this.#again.get(e.id) === told) return;
    this.#again.set(e.id, told);
    void w.observer.refetch();
  }

  async #read(key: Key, place: Place): Promise<RecordEntity> {
    const asked = key[1];
    try {
      const e = await this.ask(asked, place);
      this.#learn(asked, place, key, e);
      return e;
    } catch (err) {
      const words = this.goneOf(err);
      if (words !== null) {
        this.#gone.set(asked, words);
        queueMicrotask(() => this.client.removeQueries({ queryKey: ['record', asked], type: 'inactive' }));
      }
      throw err;
    }
  }

  /** What an answer says beside itself: the epoch its place reads under, and where an id went. Its entity goes under its own key,
   *  and every watcher that now has one moves to it. */
  #learn(asked: string, place: Place, key: Key, e: RecordEntity) {
    this.#epochs.set(placeKey(place), e.epoch);
    if (e.id !== asked) this.#went.set(asked, e.id);
    this.#gone.delete(e.id);
    const own = recordKey(e.id, e.epoch);
    if (same(own, key)) return;
    this.client.setQueryData(own, e);
    this.#move();
  }

  /** Each watcher onto its record's key, where its place's epoch is known or its record is another now. */
  #move() {
    for (const w of this.#watchers) {
      const key = this.#keyOf(w.asked, w.place);
      if (same(key, w.key)) continue;
      if (!this.#epochs.has(placeKey(w.place)) && key[1] === w.key[1]) continue;
      w.key = key;
      w.observer.setOptions(this.#options(w));
    }
  }

  /** Records told changed (the live channel's `changed { rev, ids }`, the change log's `record` kind), or all stale (a burst). */
  told(t: Told) {
    if (t.stale === 'record') return void this.client.invalidateQueries({ queryKey: ['record'] });
    let moved = false;
    for (const c of t.ids ?? []) {
      if (t.rev !== undefined) this.#told.set(c.id, Math.max(this.#told.get(c.id) ?? 0, t.rev));
      if (c.op === 'merged' && c.into) {
        this.#went.set(c.id, c.into);
        moved = true;
        continue;
      }
      // Changed, or forgotten: what is on screen asks again (a 404 then says it is gone), the rest is marked.
      void this.client.invalidateQueries({ queryKey: ['record', c.id], predicate: (q) => !q.state.data || t.rev === undefined || (q.state.data as RecordEntity).rev < t.rev });
    }
    if (!moved) return;
    this.#move();
    for (const c of t.ids ?? []) if (c.op === 'merged') this.client.removeQueries({ queryKey: ['record', c.id] });
  }

  /** The change log can't say what changed (a `since` before its floor, LV9): every record on screen is read again, the rest dropped. */
  reset() {
    this.client.removeQueries({ queryKey: ['record'], type: 'inactive' });
    void this.client.invalidateQueries({ queryKey: ['record'] });
  }

  /** The viewer's role, rights or settings changed (LV4, CN10): every epoch anew, every record on screen read again. */
  account() {
    this.#epochs.clear();
    void this.client.invalidateQueries({ queryKey: ['record'] });
  }

  /** An action on a record was refused (CN10, RS19): what it offered may no longer hold, so the record is read again. */
  refused(id: string) {
    void this.client.invalidateQueries({ queryKey: ['record', this.resolve(id)] });
  }

  /** An action on a record answered: read again at once, at or after the rev its answer names where it names one (RS22, GF's
   *  `{ result, undo?, rev }`). */
  after(id: string, rev?: number | null) {
    const now = this.resolve(id);
    if (typeof rev === 'number') this.told({ rev, ids: [{ id: now, op: 'changed' }] });
    else this.refused(now);
  }
}

/** A `changed` heard (the live channel's, or the change log's told the same way): an account's rights read again with `/api/me`
 *  (`readMe`), records told, the log's reset. Another area's is not the cache's. */
export function onChanged(cache: RecordCache, c: Changed, readMe: () => void) {
  if (c.area === 'account') {
    readMe();
    return cache.account();
  }
  if (c.area) return;
  if (c.reset === 'record') return cache.reset();
  if (c.stale === 'record' || c.ids) cache.told(c);
}

/** How long what was shown stays while its next read is under way, before the standard loader (LV7). */
export const KEEP_MS = 300;
/** The one loading rule (LV7, M5), `ms` after the sheet asked: the record as it is when current; a stale one kept up to 300 ms,
 *  then the loader; nothing before 300 ms where nothing is cached; the existing notice where it is gone or failed. */
export function look(s: Shown, ms: number): 'entity' | 'kept' | 'loader' | 'none' | 'gone' | 'error' {
  if (s.gone) return 'gone';
  if (s.entity && s.current) return 'entity';
  if (s.error && !s.fetching) return 'error';
  if (ms < KEEP_MS) return s.entity ? 'kept' : 'none';
  return 'loader';
}
