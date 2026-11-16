// One offline mode for what failed (0.17; the owner, 2026-11-08: "a unified way of handling offline mode"): with Cratarr out of reach
// a page the device cannot answer says so in one line, "‹Page› needs Cratarr.", in place of its failure's Notice and Try again; a part
// of a page whose one line is said elsewhere says nothing. Reached, a failure keeps its Notice. Pure: lib/Notice reads it.

/** What a failure shows: its Notice with Try again, the one line (`away`) out of reach, or nothing. `line`: what it says out of reach;
 *  none: it has no such line, and keeps its Notice (a sheet's, a menu's); '': a part of a page, whose line is the page's. */
export function noticeShows(away: boolean, line: string | undefined): 'notice' | 'line' | 'none' {
  if (!away || line === undefined) return 'notice';
  return line ? 'line' : 'none';
}
