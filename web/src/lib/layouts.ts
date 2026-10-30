// The layouts a list is shown in, as chosen in View (lib/ViewSheet) and as they can be here (the owner, 2026-10-26):
// no table on a phone's screen, where it adds nothing (rows instead, chips for the playlists), and neither Covers nor Flow
// for the Library's tracks (rows instead). The choice itself is kept, so a tablet still shows its table.
import { phoneScreen } from './device';
import { ALBUM_LAYOUTS, PLAYLIST_LAYOUTS } from './pageView.svelte';
import { prefs, type AlbumLayout } from './prefs.svelte';

export type LibraryList = 'albums' | 'artists' | 'tracks';

const fits = (list: LibraryList, l: AlbumLayout) => !(phoneScreen && l === 'table') && !(list === 'tracks' && (l === 'grid' || l === 'flow'));
/** The layouts View offers for a Library list. */
export const libraryLayouts = (list: LibraryList) => ALBUM_LAYOUTS.filter((o) => fits(list, o.value));
/** The layout a Library list is shown in: the chosen one where it can be, else rows. */
export const libraryLayout = (list: LibraryList): AlbumLayout => (fits(list, prefs.albumLayout) ? prefs.albumLayout : 'rows');
/** The layouts View offers for the playlists. */
export const playlistLayouts = () => PLAYLIST_LAYOUTS.filter((o) => !(phoneScreen && o.value === 'table'));
/** The playlists' layout: a table as chips on a phone. */
export const playlistLayout = () => (phoneScreen && prefs.playlistLayout === 'table' ? 'chips' : prefs.playlistLayout);
