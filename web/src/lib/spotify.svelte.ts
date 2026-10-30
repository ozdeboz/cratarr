// The signed-in account's Spotify library (decision 0073; the owner, 2026-10-27: to see what of it is here and ask for
// what is not; no playing on Spotify): from the export it uploaded (Import, anyone) or its own app's live connection
// (Account › Spotify, Premium). Its state, read once and again on asking; Discover's Spotify views (its picker's
// group, shown once it has lists) and the Filter's switches for them.
import { getSpotify, type SpotifyState, type SpotifyStatus } from './api';
import { isSpotifyView } from './discoverModes';
import { switchOn } from './listFilter.svelte';
import { prefs } from './prefs.svelte';

export const spotify = $state({ state: null as SpotifyState | null });

let asking: Promise<void> | null = null;
/** Its state read (again when `fresh`); one read at a time. */
export function loadSpotify(fresh = false): Promise<void> {
  if (spotify.state && !fresh) return Promise.resolve();
  asking ??= getSpotify()
    .then((s) => void (spotify.state = s))
    .catch(() => {})
    .finally(() => (asking = null));
  return asking;
}

/** Discover shows one of Spotify's lists now (the Library's before 2026-10-29). */
export const inSpotify = () => isSpotifyView(prefs.discoverMode);
/** The Filter's kind of list for them (lib/pageView SWITCHES). */
export const SPOTIFY_FILTER = 'spotify';

/** What the Filter leaves out of Spotify's lists (its Leave out switches). */
export function spotifyPasses(status: SpotifyStatus): boolean {
  if (status === 'in') return !switchOn(SPOTIFY_FILTER, 'no-in');
  if (status === 'partly') return !switchOn(SPOTIFY_FILTER, 'no-partly');
  if (status === 'requested') return !switchOn(SPOTIFY_FILTER, 'no-requested');
  return true;
}
