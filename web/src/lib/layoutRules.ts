// The layout a list is shown in (lib/layouts), from what View chose, the screen and the page's search: apart from
// Svelte's state, so it can be tested.

export type ShownLayout = 'rows' | 'grid' | 'table' | 'flow';
export type LibraryList = 'albums' | 'composers';
export type PlaylistLayout = 'chips' | 'table';

/** Whether a layout can show a Library list (the owner, 2026-10-26): no table on a phone's screen, where it adds
 *  nothing; no Flow for Classical's composers (a coverflow of portraits, most of them missing). */
export const layoutFits = (list: LibraryList, l: ShownLayout, phone: boolean) => !(phone && l === 'table') && !(list === 'composers' && l === 'flow');

/** The layout a Library list shows in: the table while the page's search has words (the owner, 2026-10-31: filtered
 *  results as the table, a phone's too), else the chosen one where it fits, else rows. View's choice is never changed. */
export const libraryShownOf = (list: LibraryList, chosen: ShownLayout, phone: boolean, searching: boolean): ShownLayout =>
  searching ? 'table' : layoutFits(list, chosen, phone) ? chosen : 'rows';

/** The playlists' layout: the table while the page's search has words (a phone's too), else the chosen one, a table
 *  as chips on a phone. */
export const playlistShownOf = (chosen: PlaylistLayout, phone: boolean, searching: boolean): PlaylistLayout =>
  searching ? 'table' : phone && chosen === 'table' ? 'chips' : chosen;
