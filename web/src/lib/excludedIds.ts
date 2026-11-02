// The tracks excluded everywhere (decision 0024, amended), as a track's menu needs them (the owner, 2026-10-31: "Exclude
// this track everywhere" a switch beside the artist's): each source's excluded ids as read, so the switch shows the
// truth. A source not read yet says nothing (null): the switch waits for it rather than guessing.

/** Each source's excluded track ids, as read (and changed here since). */
export type ExcludedIds = Readonly<Record<string, readonly string[]>>;

/** Whether a track is excluded everywhere: null while its source's list is not read. */
export const excludedIn = (lists: ExcludedIds, source: string, id: string): boolean | null =>
  source in lists ? lists[source].includes(id) : null;

/** A source's list with `ids` excluded (`on`) or back; an unread list stays unread (nothing to change it from). */
export function withExcluded(lists: ExcludedIds, source: string, ids: readonly string[], on: boolean): ExcludedIds {
  if (!(source in lists)) return lists;
  const rest = lists[source].filter((x) => !ids.includes(x));
  return { ...lists, [source]: on ? [...rest, ...ids] : rest };
}
