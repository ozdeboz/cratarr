// The Library's views, named once: the picker beside back (lib/LibraryViewPicker) and Settings' choice of the one it
// opens on (lib/settings/ContentPart).
export const LIBRARY_VIEWS: { value: 'albums' | 'artists' | 'tracks'; label: string }[] = [
  { value: 'albums', label: 'Albums' },
  { value: 'artists', label: 'Artists' },
  { value: 'tracks', label: 'Tracks' },
];
/** The account's Spotify lists (decision 0073, lib/spotify): the view menu's own group, once it has them. */
export const SPOTIFY_VIEWS = [
  { value: 'spotify-albums', label: 'Albums' },
  { value: 'spotify-tracks', label: 'Tracks' },
  { value: 'spotify-playlists', label: 'Playlists' },
  { value: 'spotify-artists', label: 'Artists' },
] as const;
export type SpotifyView = (typeof SPOTIFY_VIEWS)[number]['value'];
export const isSpotifyView = (v: unknown): v is SpotifyView => SPOTIFY_VIEWS.some((x) => x.value === v);
