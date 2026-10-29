// The Library's views, named once: its pages in the navigation (the owner, 2026-10-29: the views as the Library's
// subpages, in the sidebar under Library and in the bar's second row) and Settings' choice of the one it opens on
// (lib/settings/ContentPart).
export const LIBRARY_VIEWS: { value: 'albums' | 'artists' | 'tracks'; label: string }[] = [
  { value: 'albums', label: 'Albums' },
  { value: 'artists', label: 'Artists' },
  { value: 'tracks', label: 'Tracks' },
];
/** The account's Spotify lists (decision 0073, lib/spotify): more of the Library's pages, after its own, once it has
 *  them (the owner, 2026-10-29). */
export const SPOTIFY_VIEWS = [
  { value: 'spotify-albums', label: 'Albums' },
  { value: 'spotify-tracks', label: 'Tracks' },
  { value: 'spotify-playlists', label: 'Playlists' },
  { value: 'spotify-artists', label: 'Artists' },
] as const;
export type SpotifyView = (typeof SPOTIFY_VIEWS)[number]['value'];
export const isSpotifyView = (v: unknown): v is SpotifyView => SPOTIFY_VIEWS.some((x) => x.value === v);
export type LibraryView = (typeof LIBRARY_VIEWS)[number]['value'] | SpotifyView;

/** The Library's pages as the navigation shows them: its own three, then the Spotify lists where `spotify`. */
export const libraryPages = (spotify: boolean): { value: LibraryView; label: string }[] => [
  ...LIBRARY_VIEWS,
  ...(spotify ? SPOTIFY_VIEWS.map((v) => ({ value: v.value, label: `Spotify ${v.label.toLowerCase()}` })) : []),
];
/** A Library page's address. */
export const libraryHref = (v: LibraryView) => `#/library/${v}`;
/** The Library page an address asks for (`#/library/tracks`), if it names one. */
export function libraryViewOf(hash: string): LibraryView | null {
  const v = /^#\/library\/([a-z-]+)$/.exec(hash.split('?')[0])?.[1];
  return v && (LIBRARY_VIEWS.some((x) => x.value === v) || isSpotifyView(v)) ? (v as LibraryView) : null;
}
