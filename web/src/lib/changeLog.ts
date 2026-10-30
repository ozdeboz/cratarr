// Cratarr's change log as the page reads it (decision 0087): what changed since the last change seen (`rev`), for the
// kinds the page shows (playlists, favourites, ratings, what is excluded), each to read again. A `since` older than the
// log keeps answers `reset`: every kind is read again whole. The fetching and the reading again are lib/changes.

export type Change = { rev: number; kind: string; id: string; op: 'changed' | 'deleted' };
export type ChangesAnswer = { rev: number; changes?: Change[]; more?: boolean; reset?: boolean };

/** Per kind, the ids to read again, or 'all'. */
export type Refresh = Map<string, Set<string> | 'all'>;

/** One answer read into `into`: the rev to ask from next, and whether more waits behind it. Kinds not asked for are
 *  passed over. */
export function readChanges(kinds: readonly string[], answer: ChangesAnswer, into: Refresh = new Map()): { rev: number; more: boolean; refresh: Refresh } {
  if (answer.reset) {
    for (const k of kinds) into.set(k, 'all');
    return { rev: answer.rev, more: false, refresh: into };
  }
  for (const c of answer.changes ?? []) {
    if (!kinds.includes(c.kind)) continue;
    const now = into.get(c.kind);
    if (now === 'all') continue;
    const set = now ?? new Set<string>();
    set.add(c.id);
    into.set(c.kind, set);
  }
  return { rev: answer.rev, more: !!answer.more, refresh: into };
}

/** A favourite's id in the log, `track:<source>/<id>`, `album:<source>/<id>`, `artist:<source>/<name>` or
 *  `playlist:<id>`, as its parts; null for one the page does not know. */
export function favouriteOf(id: string): { what: 'track' | 'album' | 'artist'; source: string; id: string } | { what: 'playlist'; id: string } | null {
  const colon = id.indexOf(':');
  if (colon < 0) return null;
  const what = id.slice(0, colon);
  const rest = id.slice(colon + 1);
  if (what === 'playlist') return rest ? { what, id: rest } : null;
  if (what !== 'track' && what !== 'album' && what !== 'artist') return null;
  const slash = rest.indexOf('/');
  if (slash <= 0 || slash === rest.length - 1) return null;
  return { what, source: rest.slice(0, slash), id: rest.slice(slash + 1) };
}
