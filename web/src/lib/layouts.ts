// The layouts a list is shown in, as chosen in View (lib/ViewSheet) and as they can be here (lib/layoutRules; the
// owner, 2026-10-26): no table on a phone's screen, where it adds nothing (rows instead, chips for the playlists), and
// neither Covers nor Flow for the Library's tracks (rows instead), no Flow for Classical's composers. The choice itself
// is kept, so a tablet still shows its table. While the page's search has words its results are the table, a phone's
// too (the owner, 2026-10-31); View still offers and shows what was chosen.
import { phoneScreen } from './device';
import { layoutFits, libraryShownOf, playlistShownOf, type LibraryList } from './layoutRules';
import { ALBUM_LAYOUTS, PLAYLIST_LAYOUTS, pageView } from './pageView.svelte';
import { prefs, type AlbumLayout } from './prefs.svelte';

export type { LibraryList } from './layoutRules';

/** The pages whose lists turn to the table while searching: the Library (and the device's own albums, its shelf) and
 *  the playlists; their place before the search comes back with the list (App). */
export const searchesAsTable = (page: string) => page === 'library' || page === 'local-albums' || page === 'playlists';
/** Words in the page's search. */
const searching = () => pageView.query.trim() !== '';
/** The layouts View offers for a Library list. */
export const libraryLayouts = (list: LibraryList) => ALBUM_LAYOUTS.filter((o) => layoutFits(list, o.value, phoneScreen));
/** The layout chosen for a Library list as it can be here (View's): the chosen one where it fits, else rows. */
export const libraryLayout = (list: LibraryList): AlbumLayout => libraryShownOf(list, prefs.albumLayout, phoneScreen, false);
/** The layout a Library list is shown in: the table while searching, else View's. */
export const libraryLayoutShown = (list: LibraryList): AlbumLayout => libraryShownOf(list, prefs.albumLayout, phoneScreen, searching());
/** The layouts View offers for the playlists. */
export const playlistLayouts = () => PLAYLIST_LAYOUTS.filter((o) => !(phoneScreen && o.value === 'table'));
/** The playlists' layout as View shows it: a table as chips on a phone. */
export const playlistLayout = () => playlistShownOf(prefs.playlistLayout, phoneScreen, false);
/** The layout the playlists are shown in: the table while searching, else View's. */
export const playlistLayoutShown = () => playlistShownOf(prefs.playlistLayout, phoneScreen, searching());
