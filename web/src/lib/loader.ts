// One loading rule (0.13; the owner, 2026-10-31: "Keep, then spinner at 300 ms", one rule for every list and place):
// while what replaces a list's or a place's content is on its way, what it showed stays as it was (not blanked, not
// dimmed) for LOADER_AFTER_MS; a quicker answer just takes its place, a slower one finds the loader there (the mark in
// motion, lib/MarkLoader via lib/Asking or lib/PageLoading). Never a blank gap, never "none found" before the answer.
// The decision here; lib/keepWhile.svelte keeps the content and times it.

/** How long what was shown stays before the loader takes its place, and how long anything waited for may take before a
 *  sign of the wait shows (the progress bar, an opened cover's ring). One number for all of them. */
export const LOADER_AFTER_MS = 300;

/** What a place shows `now` (ms) when what replaces its content has been waited for since `since` (ms; null: nothing
 *  waited for, the answer is in): the answer; what it showed, kept; or the loader. */
export function waitShows(since: number | null, now: number): 'answer' | 'kept' | 'loader' {
  if (since === null) return 'answer';
  return now - since < LOADER_AFTER_MS ? 'kept' : 'loader';
}

/** The loader's size where it is (the owner, 2026-11-02: "we're using smaller kinds these days in smaller panels", and
 *  Home "has no big loading spinner in the middle"): a page's large; in a panel (beside the page or over it) or a
 *  sheet, small. One rule for every loader of a place (lib/PageLoading, lib/Hold's sign). */
export const LOADER_LARGE = 96;
export const LOADER_SMALL = 40;
export const loaderSizeIn = (el: Element): number => (el.closest('.player-side, .playlist-layer, .sheet') ? LOADER_SMALL : LOADER_LARGE);

/** The room under a page's loader for the line of what it waits for (lib/PageLoading's `.under`: 16 px under the mark, then a
 *  line), kept whether a line shows or not, so the line coming moves nothing. */
export const LOADER_LINE = 40;

/** Where a page's loader sits (lib/PageLoading): its height and its mark's offset from its top, from what can be seen of its
 *  container (`top`, `bottom`), where the loader starts in it (`at`), its mark's size and the room the line under the mark takes
 *  (`under`, measured; LOADER_LINE at least). The mark in the middle of its own room, as much room above it as below, and lower
 *  only where what comes before it reaches that far; the line's room counted in the height (the review of 1.2, low 12: on a phone
 *  the words hung past the box), so the line always ends inside it. */
export function loaderFit(v: { top: number; bottom: number; at: number; mark: number; under: number }): { height: number; top: number } {
  const room = Math.max(LOADER_LINE, v.under);
  const middle = Math.max((Math.max(v.top, v.at) + v.bottom) / 2, v.at + v.mark / 2);
  const height = Math.round(Math.max(v.bottom - v.at, v.mark * 2 + room));
  return { height, top: Math.round(Math.max(0, Math.min(middle - v.at - v.mark / 2, height - v.mark - room))) };
}
