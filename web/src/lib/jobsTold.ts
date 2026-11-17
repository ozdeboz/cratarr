// Who is told of a finished conversion (decision 0013; W13, X5: a user may undo their own Incoming actions). The queue comes to
// every page on the live channel; an administrator is told of every job that ends, a user only of those their own page queued
// (an Add to of theirs that waited for its files to be converted), each with its Undo, which is theirs. A job does not say who
// queued it, so a page knows its own by the record and the copy it asked to move. Pure.

/** A conversion this page queued: the record (its source and id) and the copy it works on, where the page named one. */
export type Queued = { source: string; album_id: string; from?: string | null };

/** Whether a job is one this page queued: its record, and its copy where both name one. */
export const queuedHere = (mine: readonly Queued[], j: Queued) =>
  mine.some((m) => m.source === j.source && m.album_id === j.album_id && (!m.from || !j.from || m.from === j.from));

/** The ended jobs to tell of: every one to an administrator, a user's own to a user. */
export function toldOf<J extends Queued>(ended: readonly J[], admin: boolean, mine: readonly Queued[]): J[] {
  return admin ? [...ended] : ended.filter((j) => queuedHere(mine, j));
}
