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
