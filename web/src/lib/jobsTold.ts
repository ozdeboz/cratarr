// Who is told of a finished conversion (decision 0013; W13, X5: a user may undo their own Incoming actions). The queue comes to
// every page on the live channel, to a user their own alone (G8); an administrator is told of every job that ends, a user of
// those they queued (an Add to of theirs that waited for its files to be converted), from any of their pages, each with its Undo,
// which is theirs. A job says who queued it (`by`); one that does not is known by the record and the copy this page asked to move.
// Pure.

/** A conversion this page queued: the record (its source and id) and the copy it works on, where the page named one. */
export type Queued = { source: string; album_id: string; from?: string | null };

/** Whether a job is one this page queued: its record, and its copy where both name one. */
export const queuedHere = (mine: readonly Queued[], j: Queued) =>
  mine.some((m) => m.source === j.source && m.album_id === j.album_id && (!m.from || !j.from || m.from === j.from));

/** The ended jobs to tell of: every one to an administrator, a user's own to a user: one saying who queued it is theirs (the
 *  server sends a user their own alone), else one this page queued. */
export function toldOf<J extends Queued & { by?: string | null }>(ended: readonly J[], admin: boolean, mine: readonly Queued[]): J[] {
  return admin ? [...ended] : ended.filter((j) => !!j.by || queuedHere(mine, j));
}
