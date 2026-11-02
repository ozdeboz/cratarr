// Home's answers as last shown (the owner, 2026-11-01: coming back to Home loaded it all again from scratch; 0.13's
// one loading rule: keep what was shown). Home shows them at once on its return and asks again quietly, each replaced
// as its answer comes; an answer that fails keeps what was shown. Kept while the app runs, for one server and one set
// of libraries switched off: another choice starts empty.
import type { Digging, HomeBack, HomeWeek, ListedAlbum, Moment, PlaylistRow } from './api';

export type CrateShelf = { name: string; source: string; albums: { id: string; title: string; artist: string | null; year: number | null }[] };

const empty = () => ({
  newest: null as { source: string; a: ListedAlbum }[] | null,
  /** undefined: not known yet; null: none. */
  week: undefined as HomeWeek | null | undefined,
  moment: undefined as Moment | null | undefined,
  /** When the moment was asked (ms), and the part of the day it was. */
  momentAt: 0,
  momentPart: '',
  back: null as HomeBack[] | null,
  dug: undefined as Digging | null | undefined,
  favAlbumIds: null as string[] | null,
  favRows: null as PlaylistRow[] | null,
  crates: {} as Record<string, CrateShelf | null>,
  /** The header's picture was in once: no wait for it again. */
  sky: false,
  /** Home was shown once: on its return each section shows as its answer comes, none waiting for those above it. */
  seen: false,
});

let keptFor = '';
let kept = $state(empty());

/** The part of the day, as the moment changes with it: morning, day, evening, night. */
export function partOfDay(d = new Date()): string {
  const h = d.getHours();
  return h >= 5 && h < 12 ? 'morning' : h >= 12 && h < 18 ? 'day' : h >= 18 && h < 23 ? 'evening' : 'night';
}
/** Whether the kept moment still holds (the owner, 2026-11-01: checked before it shows): asked within half an hour,
 *  in the same part of the day. It follows the weather and the time; the other sections do not go stale in minutes. */
export const momentFresh = (k: { moment: unknown; momentAt: number; momentPart: string }, now = Date.now()) =>
  k.moment !== undefined && now - k.momentAt < 30 * 60_000 && k.momentPart === partOfDay(new Date(now));

/** What Home showed last, for this server and these libraries (emptied for another). */
export function homeKept(source: string, without: readonly string[] | undefined) {
  const key = JSON.stringify([source, without ?? []]);
  if (key !== keptFor) {
    keptFor = key;
    kept = empty();
  }
  return kept;
}
