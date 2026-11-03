// Modes per server (decision 0098): a Jellyfin or a Subsonic server in the app, as Local is (the owner, 2026-11-03: "In
// the app, as Local"). The app signs in to it and streams from it itself; Cratarr's server never sees it. This is what
// every such server answers, whichever kind: its library, search, Home's lists, its playlists (edited there), its own
// mix for Radio and Autoplay, its stars and its play reports, and the links its streams and covers are at. The page's
// views are built from these as local mode's are from the device's tracks (lib/deviceMusic).

/** The kinds of server a mode can be. */
export type ServerKind = 'jellyfin' | 'subsonic';

/** A server the device is signed in to, as the app keeps it (Android's keystore, the desktop app's store): never sent
 *  to Cratarr's server. `secret` is Jellyfin's access token, or the Subsonic password or API key (`apiKey`). */
export type ServerSignIn = {
  /** The device's own id for it, the mode's name in the app (`server:<id>`). */
  id: string;
  kind: ServerKind;
  /** The server's own name, as it says it. */
  name: string;
  url: string;
  user: string;
  /** Jellyfin's id of the user. */
  userId?: string;
  secret: string;
  /** The secret is an OpenSubsonic API key, not a password. */
  apiKey?: boolean;
  /** "This server scrobbles for me" (decision 0091, amended; off at the start): the device leaves its tracks'
   *  scrobbles to the server, and only reports their plays to it. */
  scrobbles?: boolean;
};

/** A track of the server's. `entry`: its place in a playlist, as that server names one (Jellyfin's playlist item id,
 *  Subsonic's index), only in a playlist's tracks. */
export type ServerTrack = {
  id: string;
  name: string;
  artist: string | null;
  artist_id: string | null;
  album: string | null;
  album_id: string | null;
  track: number | null;
  disc: number | null;
  year: number | null;
  duration_secs: number | null;
  /** Epoch seconds. */
  added: number | null;
  starred: boolean;
  /** Its cover's id on the server (its album's, mostly). */
  cover: string | null;
  entry?: string;
};

export type ServerAlbum = {
  id: string;
  title: string;
  artist: string | null;
  artist_id: string | null;
  year: number | null;
  added: number | null;
  tracks: number;
  duration_secs: number | null;
  starred: boolean;
  cover: string | null;
};

export type ServerArtist = { id: string; name: string; albums: number | null; starred: boolean; cover: string | null };

export type ServerPlaylist = { id: string; name: string; tracks: number; duration_secs: number | null; cover: string | null };

/** Home's rows in a server's mode (the owner, 2026-11-03: "That server's lists"). */
export type HomeList = 'added' | 'played' | 'most' | 'favourites' | 'random';
export const HOME_LISTS: readonly HomeList[] = ['added', 'played', 'most', 'favourites', 'random'];
/** Their names, as Home and Settings › Content say them. */
export const HOME_LIST_NAMES: Record<HomeList, string> = { added: 'Recently added', played: 'Recently played', most: 'Most played', favourites: 'Favourites', random: 'Random' };

export type AlbumOrder = 'name' | 'artist' | 'added' | 'year';

export interface MusicServer {
  readonly kind: ServerKind;
  readonly signIn: ServerSignIn;

  /** One page of the library's albums, in an order. */
  albums(order: AlbumOrder, offset: number, size: number): Promise<ServerAlbum[]>;
  album(id: string): Promise<{ album: ServerAlbum; tracks: ServerTrack[] }>;
  artists(): Promise<ServerArtist[]>;
  artist(id: string): Promise<{ artist: ServerArtist; albums: ServerAlbum[] }>;
  search(words: string, size: number): Promise<{ artists: ServerArtist[]; albums: ServerAlbum[]; tracks: ServerTrack[] }>;
  /** One of Home's rows. */
  list(which: HomeList, size: number): Promise<ServerAlbum[]>;

  playlists(): Promise<ServerPlaylist[]>;
  playlist(id: string): Promise<{ playlist: ServerPlaylist; tracks: ServerTrack[] }>;
  /** A new playlist with these tracks; its id. */
  createPlaylist(name: string, tracks: string[]): Promise<string>;
  renamePlaylist(id: string, name: string): Promise<void>;
  addToPlaylist(id: string, tracks: string[]): Promise<void>;
  /** The playlist's tracks at these entries taken out. */
  removeFromPlaylist(id: string, entries: string[]): Promise<void>;
  /** The playlist as these of its entries, in this order: those left out taken out (the panel's edit mode, saved). */
  keepPlaylist(id: string, entries: string[]): Promise<void>;
  deletePlaylist(id: string): Promise<void>;

  /** The server's own mix from a track (the owner, 2026-11-03: Radio and Autoplay are "that server's own mix"); null
   *  where the server has none, and that mode has no Radio. */
  mix(track: string, size: number): Promise<ServerTrack[] | null>;

  star(what: 'track' | 'album' | 'artist', id: string, on: boolean): Promise<void>;
  /** That it started playing here (its "now playing"), best effort. */
  playing(track: string): Promise<void>;
  /** A play, at its time (epoch seconds), as that server counts plays. */
  played(track: string, at: number): Promise<void>;

  /** Where the player streams a track from, with the sign-in in it: never shown, never logged. */
  streamUrl(track: string): Promise<string>;
  coverUrl(cover: string, size: number): Promise<string>;
}

/** A server's answer refused, as the page says it (its own message where it gives one). */
export class ServerError extends Error {
  /** The sign-in was refused: the mode asks to sign in again. */
  readonly signedOut: boolean;
  constructor(message: string, signedOut = false) {
    super(message);
    this.signedOut = signedOut;
  }
}

/** Epoch seconds from a server's date, null where there is none. */
export function epoch(date: Date | string | null | undefined): number | null {
  if (!date) return null;
  const t = new Date(date).getTime();
  return Number.isFinite(t) ? Math.floor(t / 1000) : null;
}
