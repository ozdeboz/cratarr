import { track } from './progress.svelte';
import { coverKey, coverRounds, localCovers } from './localCovers.svelte';
import { asksSignIn, notReached, reached, refused, refusedFor, toSignIn, unreached } from './reach.svelte';

/** What a page says while Cratarr is out of reach (lib/reach.svelte.ts). */
export const UNREACHED = 'Cannot reach the Cratarr server';
export type Playlist = {
  id: string;
  name: string;
  member: boolean;
  /** Tracks that resolve to a file (what players show). */
  count: number;
  /** Entries that point at a missing file. */
  dead: number;
  /** Where it is kept in step (decision 0014). */
  links: Link[];
  /** Other names it is spoken as, for voice assistants (decision 0015). */
  spoken: string[];
  /** Whose it is (decision 0051), whether its owner shares it with the other users, whether it is the signed-in user's
   *  own (only its owner changes it). Left out by a server from before several users: one's own, unshared. */
  owner?: string;
  shared?: boolean;
  mine?: boolean;
};

/** A playlist's sync: "shared" (with everyone), or its copy in an account. */
export type Link = {
  /** "shared", or "user/<server>/<account>". */
  target: string;
  label: string;
  status: { at: number; error: string | null } | null;
  /** The account's copy exists (false while it waits to be made). */
  made: boolean;
};


export type NowPlaying = {
  /** The source it came from (covers are looked up there). */
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  /** The source's album id (used to mark the playing album on the Incoming page). */
  album_id: string | null;
  /** False when the track is outside the curated library, so it cannot be added to a playlist. */
  addable: boolean;
  /** Null when nothing reports a position. */
  progress: Progress | null;
  /** The player it plays on ("Finamp on Lenovo Tab"); the controls act on it. */
  player: string | null;
  /** Whether the player's account has it as a favourite; null when not known. */
  favorite: boolean | null;
  /** What plays next, in order, as far as the player reports. */
  upcoming: Upcoming[];
  /** What the server knows about the track's file and plays (the player's info chips, decision 0019). */
  info?: TrackInfo | null;
};

/** Each field null when the server does not know it. */
export type TrackInfo = {
  play_count: number | null;
  codec: string | null;
  bitrate_kbps: number | null;
  bit_depth: number | null;
  sample_rate_hz: number | null;
  explicit: boolean | null;
  /** Its tempo (decision 0035); absent from a server before it. */
  bpm?: number | null;
};

/** A track the player plays after the current one. */
export type Upcoming = {
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  addable: boolean;
  duration_secs: number | null;
  /** `next`: added by hand, ahead of the album or playlist (decision 0033's two lanes); `radio`: Radio's inserts;
   *  `autoplay`: Autoplay's, after everything (decision 0036, amended). */
  lane?: 'next' | 'radio' | 'autoplay';
};

export type Progress = {
  position_secs: number;
  duration_secs: number | null;
  paused: boolean;
  /** No player reports this track; the position is time since it started (wrong after a pause or seek). */
  estimated: boolean;
};

/** An enabled, connected source; `playing` is only filled by `getState`. */
export type SourceTab = { id: string; label: string; playing: boolean };

export type State = {
  now_playing: NowPlaying | null;
  playlists: Playlist[];
  sources: SourceTab[];
};

/** `generic`: no words from the server, only its status; callers say what failed instead (see toast.fail). `status`:
 *  the server's answer, where it gave one (a caller that tells a 404 from a 409 reads it). */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly generic = false,
    readonly status?: number,
  ) {
    super(message);
  }
}

/** A request the user is waiting for: counted for the progress bar (lib/progress.svelte.ts). */
function request<T>(path: string, init?: RequestInit): Promise<T> {
  return track(fetchJson<T>(path, init));
}

/** A request in the background (polling, checks while typing): no progress bar. */
function quietly<T>(path: string, init?: RequestInit): Promise<T> {
  // Not while a refusal holds them back (lib/reach): each would be one more knock on a ban.
  if (refusedFor() > 0) return Promise.reject(new ApiError(UNREACHED));
  return fetchJson<T>(path, init);
}

async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    unreached();
    throw new ApiError(UNREACHED);
  }
  if (asksSignIn(res)) {
    toSignIn();
    throw new ApiError(UNREACHED);
  }
  if (!res.ok) {
    let message: string | null = null;
    try {
      const body = await res.json();
      if (typeof body.error === 'string') message = body.error;
    } catch {
      /* no words from the server */
    }
    if (!message && (res.status === 403 || res.status === 401)) refused();
    if (!message && notReached(res.status)) {
      unreached();
      throw new ApiError(UNREACHED);
    }
    reached();
    throw message ? new ApiError(message, false, res.status) : new ApiError(`Server returned ${res.status}`, true, res.status);
  }
  reached();
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

/** `source`: follow that source; absent: whichever started playing most recently. */
const follow = (source?: string) => (source ? `?source=${encodeURIComponent(source)}` : '');

export const getState = (source?: string) => quietly<State>(`/api/state${follow(source)}`);

/** A track as Cratarr's players hold it (decision 0031): the local player's own description. */
export type QueueTrack = { source: string; id: string; name: string; artist: string | null; album: string | null; album_id: string | null };

/** One of Cratarr's own players (Play On, decision 0031): the Android app on a tablet or a phone. */
export type PlayOnPlayer = {
  name: string;
  /** Connected now: it takes commands. */
  online: boolean;
  /** Last heard from, epoch seconds. */
  seen: number;
  playing: boolean;
  index: number;
  position_ms: number;
  duration_ms: number | null;
  volume: number;
  ducked: boolean;
  queue_length: number;
  /** Goes up when the queue changes: read the player by name for the new one. */
  queue_version: number;
  /** Off, one or all (decision 0033, amended). */
  repeat?: 'off' | 'one' | 'all';
  shuffled?: boolean;
  /** Its sleep timer, while one is set: it pauses at `at_ms` (epoch ms) or when the playing track ends. */
  sleep?: { at_ms: number | null; end_of_track: boolean } | null;
  /** Radio's switch (decision 0036, amended), what its inserts are like and how many follow each own track. */
  radio?: boolean;
  radio_by?: 'track' | 'artist' | 'era';
  radio_count?: number;
  /** Radio's and Autoplay's picks leave the owner's favourites out. */
  skip_favourites?: boolean;
  /** Autoplay while on: its mode and switches. */
  autoplay?: { mode: string; albums: boolean; restart: boolean } | null;
  /** The group it plays in, by its leader (decision 0037): a player of a group shows what the group plays. */
  group?: string | null;
  /** How late its speaker sounds (ms): in a group it plays that much earlier. */
  delay_ms?: number;
  /** `app` (the Android app), `kodi` (a Kodi Cratarr drives: it follows a group, never leads one) or `browser` (a page
   *  that plays in a browser, a player while it is open: never in a group). */
  kind?: 'app' | 'kodi' | 'browser';
  /** Its link to Cratarr (decision 0045). */
  link?: PlayerLink;
  track: QueueTrack | null;
  queue?: QueueTrack[];
  /** Why it does not play what it was told to (its device would not let it, decision 0031 amended); null otherwise. */
  held?: string | null;
  /** Its queues (decision 0033, amended: queue history), the one that plays marked, newest first; by name only. */
  queues?: QueueItem[];
  /** Goes up when that list changes: read the player by name for the new one. */
  queues_version?: number;
};

/** A queue in a player's list (decision 0033, amended): `tracks` how many, `from` what it was started from
 *  (`playlist`, `album`, `favourites`, `library`, `tracks`, `player`), `updated` when it was left (epoch s). */
export type QueueItem = { id: string; name: string; tracks: number; from: string | null; updated: number; playing: boolean };

/** A player's link as Cratarr knows it, and as the player last told (decision 0045); times epoch seconds, the
 *  player's own epoch ms. */
export type PlayerLink = {
  offline_since: number | null;
  ended: 'replaced' | 'gone quiet' | 'closed' | 'server stopping' | null;
  ended_at: number | null;
  own: { connected_since?: number; failed: number; last_error?: string; address?: string; wait_until?: number; wait_why?: string } | null;
};

export const getPlayers = () => quietly<{ players: PlayOnPlayer[] }>('/api/players');
export const getPlayer = (name: string) => quietly<PlayOnPlayer>(`/api/players/${encodeURIComponent(name)}`);

/** A row of Cratarr's diagnostics log (decision 0045): `at` and `last` epoch ms, `count` how often it came. */
export type LogRow = {
  id: number;
  at: number;
  last: number;
  count: number;
  level: 'error' | 'warn' | 'info' | 'debug' | 'trace';
  area: string;
  place: string;
  what: string;
  fields: Record<string, unknown>;
  origin: 'server' | 'device';
};
/** Which rows, newest first: a place (a player, or "server"), an area, the least serious level, the page before a row. */
export type LogQuery = { place?: string; area?: string; level?: string; before?: number; limit?: number };
export const getLog = (q: LogQuery) => {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(q)) if (v !== undefined && v !== '') p.set(k, String(v));
  return quietly<{ rows: LogRow[] }>(`/api/diagnostics?${p}`);
};
/** What is logged by area now: the areas, the levels, each area raised for a while (`now`), until when (epoch ms). */
export type LogLevels = { areas: string[]; levels: string[]; now: Record<string, string>; until: number | null };
export const getLogLevels = () => quietly<LogLevels>('/api/diagnostics/levels');
export const setLogLevels = (areas: Record<string, string>, hours: number) =>
  request<LogLevels>('/api/diagnostics/levels', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ areas, hours }) });
/** A track's ReplayGain (decision 0035): gains in dB, peaks linear; what is not known is left out. */
export type TrackGain = { trackGain?: number; albumGain?: number; trackPeak?: number; albumPeak?: number };
export const getTrackGain = (source: string, id: string) => quietly<TrackGain>(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/gain`);
/** A Kodi Cratarr plays on: its web server, its login (the password never comes back), Cratarr's address for it. */
export type KodiSetting = { name: string; url: string; user: string; password?: string; password_set?: boolean; stream_from: string };
export type PlayOnSettings = { webhooks: string[]; kodi: KodiSetting[]; stream_default: string | null };
export const getPlayOnSettings = () => request<PlayOnSettings>('/api/players/settings');
export const savePlayOnSettings = (s: { webhooks: string[]; kodi: KodiSetting[] }) =>
  request<PlayOnSettings>('/api/players/settings', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(s) });
/** A player's speaker delay (sync play, decision 0037), 0 to 500 ms. */
export const setPlayerDelay = (name: string, ms: number) =>
  request<{ ms: number }>(`/api/players/${encodeURIComponent(name)}/delay`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ms }),
  });
/** A command for one of Cratarr's players (`{ command: 'play', tracks, index, position_ms }`, `{ command: 'pause' }`…). */
export const playerCommand = (name: string, command: Record<string, unknown>) =>
  request<void>(`/api/players/${encodeURIComponent(name)}/command`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(command),
  });

export const coverUrl = (np: { source: string; id: string }) => `/api/cover/${encodeURIComponent(np.source)}/${encodeURIComponent(np.id)}`;

/** play: in file order · shuffle: random order · next: after the current track · last: at the end of the queue. */
export type PlayMode = 'play' | 'shuffle' | 'next' | 'last';
/** sent ≤ 240 of total playable tracks; unknown = lines left out (missing, or not known to the server yet). */
export type PlayResult = { player: string; sent: number; total: number; unknown: number };

export const playPlaylist = (playlistId: string, mode: PlayMode, source?: string) =>
  post<PlayResult>(`/api/playlists/${encodeURIComponent(playlistId)}/play${follow(source)}`, { mode });

// The queue (decision 0019): one track after the current one or at the end, playing on from an upcoming track, and a
// new playlist made of what plays and what comes next (tracks in the library only).
export const queueTrack = (id: string, mode: 'next' | 'last', source?: string) =>
  post<{ player: string }>(`/api/queue/add${follow(source)}`, { id, mode });
/** Several tracks, in order (a crate's Play next / Add to queue, decision 0024). */
export const queueTracks = (ids: string[], mode: 'next' | 'last', source?: string) =>
  post<{ player: string }>(`/api/queue/add${follow(source)}`, { ids, mode });

/** A crate (decision 0024): a permanent one or one's own; `count` null when not known (the server's favourites could
 *  not be read). */
/** `favourites`: its toggle's setting; `counts`: its size with each setting (null: the favourites are not known). */
export type Crate = {
  id: string;
  name: string;
  favourites: CrateFavourites;
  counts: Record<CrateFavourites, number | null>;
  permanent: boolean;
  /** What its card shows, as its favourites toggle is set; absent where that is not known (favourites not read). */
  stats?: CrateStats;
  /** A picture of its own was set (crateImageUrl). */
  image: boolean;
};
/** A crate's card (decided 2026-10-16): its size and length, how much was never played (null: no owner account to ask
 *  about), what came this week, its top three genres and up to four of its albums (most of its tracks first). */
export type CrateStats = {
  tracks: number;
  seconds: number;
  unplayed: number | null;
  new_week: number;
  genres: string[];
  covers: { source: string; album_id: string }[];
};

/** One condition of a crate: an axis, what it is tested against, negated or not. */
export type CrateValue = { kind: 'yes' } | { kind: 'any_of'; values: string[] } | { kind: 'range'; min: number | null; max: number | null };
export type Condition = { axis: string; not: boolean; value: CrateValue };
export type CrateDef = { id: string; name: string; conditions: Condition[] };
/** An axis the builder draws a control for, with its domain in this library. */
export type Axis = {
  key: string;
  label: string;
  kind: 'yes_no' | 'values' | 'membership' | 'range';
  values: string[];
  min: number | null;
  max: number | null;
  known: number;
};
export const getCrateAxes = (source?: string) => quietly<Axis[]>(`/api/crates/axes${follow(source)}`);
export const getCrate = (id: string) => quietly<CrateDef>(`/api/crates/${encodeURIComponent(id)}`);
/** Every change answers with its undo token (`undo`, decision 0026), for `withUndo`. */
type Undo = { undo: string | null };
export const createCrate = (name: string, conditions: Condition[]) => post<{ id: string } & Undo>('/api/crates', { name, conditions });
export const updateCrate = (id: string, name: string, conditions: Condition[]) =>
  request<{ id: string } & Undo>(`/api/crates/${encodeURIComponent(id)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, conditions }),
  });
export const deleteCrate = (id: string) => request<{ deleted: string } & Undo>(`/api/crates/${encodeURIComponent(id)}`, { method: 'DELETE' });
/** The builder's live preview: how many tracks unsaved conditions select, and the first of them. */
export const previewCrate = (conditions: Condition[], limit: number, source?: string) => {
  const q = new URLSearchParams({ limit: String(limit) });
  if (source) q.set('source', source);
  return quietly<{ source: string; total: number; rows: PlaylistRow[] }>(`/api/crates/preview?${q}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ conditions }),
  });
};
export const getCrates = (source?: string) => quietly<Crate[]>(`/api/crates${follow(source)}`);
/** A page of a crate's rows (newest first) and the crate's size. */
/** A crate's own picture (kept as JPEG within 800 px); `round` reads it again after a change. */
export const crateImageUrl = (id: string, round = 0) => `/api/crates/${encodeURIComponent(id)}/image${round ? `?v=${round}` : ''}`;
/** Sets it: any JPEG, PNG, GIF or WebP, up to 32 MB. */
export const setCrateImage = (id: string, file: Blob) =>
  request<void>(`/api/crates/${encodeURIComponent(id)}/image`, { method: 'PUT', headers: { 'Content-Type': file.type || 'application/octet-stream' }, body: file });
export const removeCrateImage = (id: string) => request<void>(`/api/crates/${encodeURIComponent(id)}/image`, { method: 'DELETE' });
/** The crate panel's favourites toggle (decision 0024, amended): all its tracks, only the favourites, or none. */
export type CrateFavourites = 'all' | 'only' | 'none';
/** `favourites`: the toggle's setting as the page shows it (a tap may be on its way); the saved one without. */
export const crateRows = (id: string, offset: number, limit: number, source?: string, favourites?: CrateFavourites) => {
  const q = new URLSearchParams({ offset: String(offset), limit: String(limit) });
  if (favourites) q.set('favourites', favourites);
  if (source) q.set('source', source);
  return quietly<{ source: string; total: number; rows: PlaylistRow[]; genres: string[] }>(`/api/crates/${encodeURIComponent(id)}/rows?${q}`);
};
/** A crate's tempo spread (decision 0041): its tracks' 5-BPM bands, 60 to 180, counted on the server. */
export type TempoBands = { bands: number[]; with_tempo: number; total: number };
export const crateTempo = (id: string, source?: string, favourites?: CrateFavourites) => {
  const q = new URLSearchParams();
  if (favourites) q.set('favourites', favourites);
  if (source) q.set('source', source);
  return quietly<TempoBands>(`/api/crates/${encodeURIComponent(id)}/tempo?${q}`);
};
/** A playlist's facts for the Playlists page's table (decision 0042): its length, top genres, how much the owner never
 *  played (null: no owner account) and when they last played any of it, and when its file last changed (epoch s). */
export type PlaylistStats = { seconds: number; genres: string[]; unplayed: number | null; last_played: number | null; changed: number };
export const playlistStats = () => quietly<Record<string, PlaylistStats>>('/api/playlists/stats');
/** Make playlist: the crate's tracks now, as its favourites toggle is set, as a new ordinary playlist (a snapshot). */
export const crateToPlaylist = (id: string, name: string, source?: string) =>
  post<{ playlist: string; added: number } & Undo>(`/api/crates/${encodeURIComponent(id)}/playlist${follow(source)}`, { name });
/** A crate's favourites toggle, kept on the server for every device. */
export const setCrateFavourites = (id: string, favourites: CrateFavourites) =>
  request<{ crate: string; favourites: CrateFavourites }>(`/api/crates/${encodeURIComponent(id)}/favourites`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ favourites }),
  });
export const jumpTo = (id: string, source?: string) => post<{ player: string }>(`/api/queue/jump${follow(source)}`, { id });
/** `ids`: the queue to make it from (the local player's); without, the server's player's queue. `repeats`: a track
 *  the queue plays twice is in it twice (the page asked), else once (decision 0025, amended 2026-10-16). */
export const playlistFromQueue = (name: string, source?: string, ids?: string[], repeats = false) =>
  post<{ playlist: string; added: number; left_out: number } & Undo>(`/api/queue/playlist${follow(source)}`, ids ? { name, ids, repeats } : { name, repeats });

/** Mark the playing track a favourite or not. Says the state wanted, so a retried request changes nothing more. */
export const setFavorite = (on: boolean, source?: string) => {
  const q = new URLSearchParams({ on: String(on) });
  if (source) q.set('source', source);
  return quietly<State>(`/api/favorite?${q}`, { method: 'POST' }); // shown at once (optimistic)
};

/** Put the playing track in the playlist or take it out. Says the state wanted, so a retried request changes nothing more. */
export const setMember = (playlistId: string, member: boolean, source?: string) => {
  const q = new URLSearchParams({ member: String(member) });
  if (source) q.set('source', source);
  return quietly<State>(`/api/playlists/${encodeURIComponent(playlistId)}/toggle?${q}`, { method: 'POST' }); // shown at once
};

export type Repair = { playlist: string; from: string; to: string };
export type Repairs = { safe: Repair[]; review: Repair[] };

export const getRepairs = () => request<Repairs>('/api/repairs');

const post = <T>(path: string, body: unknown) =>
  request<T>(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

/** The radio's next tracks (decision 0036), already in the radio's lane: like `seed` (the radio's seed, or the queue's
 *  own album or playlist), none of `exclude` (the queue), after `last` (the playing track). */
export const radioNext = (ask: {
  mode: string;
  seed: string[];
  exclude: string[];
  last: string | null;
  count: number;
  within?: string | null;
  albums?: boolean;
  /** Radio's inserts: what they are like (the track, its artist, its era). */
  by?: string;
  /** Leave the owner's favourites out. */
  skip_favourites?: boolean;
}) =>
  post<{ tracks: QueueTrack[] }>('/api/radio/next', ask);

/** Apply every safe repair, or only those of these playlists (the panel's Repair tab, decision 0025). */
export const applySafe = (playlists?: string[]) =>
  post<{ playlists: string[] } & Undo>('/api/repairs/apply', playlists ? { kind: 'safe', playlists } : { kind: 'safe' });

/** Apply one offered repair (used for the ones that need a look). */
export const applyOne = (r: Repair) => post<{ playlists: string[] } & Undo>('/api/repairs/apply', { kind: 'one', ...r });

/** The one undo (decision 0026): an action's tokens, undone newest first; the redo's tokens come back (none for
 *  Incoming's, whose albums' titles come back instead). */
export const undo = (tokens: string[]) => post<{ redo: string[]; restored: string[] }>('/api/undo', { tokens });


export const createPlaylist = (name: string) => post<{ playlist: string } & Undo>('/api/playlists', { name });

/** What to export (decision 0007): playlists by name, with their tracks or not, the tracks in one folder or the
 *  library's, and the playlists as they are or combined into one of this name. */
export type ExportChoice = { playlists: string[]; tracks: boolean; flat: boolean; combine: string | null };
/** What an export will hold, before starting: `bytes` only with tracks; missing tracks are left out. */
export type ExportPreview = { tracks: number; missing: number; bytes: number | null };
export const exportPreview = (c: ExportChoice) =>
  quietly<ExportPreview>('/api/export/preview', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(c) });
/** The download itself: a link, so the browser shows its progress and can cancel it. */
export function exportUrl(c: ExportChoice): string {
  const q = new URLSearchParams();
  for (const p of c.playlists) q.append('playlist', p);
  if (c.tracks) q.set('tracks', '1');
  if (c.tracks && c.flat) q.set('flat', '1');
  if (c.combine !== null) q.set('combine', c.combine);
  return `/api/export?${q}`;
}

/** A playlist's first albums (up to four, in playlist order), for the mosaic in its menu (decision 0021). */
/** One line of a playlist for its view (decision 0023): what the catalogue knows of its file; `id` null when not matched
 *  to the server, `missing` when the file is not in the Music folder. */
export type PlaylistRow = {
  entry: string;
  file: string;
  id: string | null;
  name: string | null;
  artist: string | null;
  album: string | null;
  duration_secs: number | null;
  missing: boolean;
  /** A playlist's rows, for the panel's orders: from the file's tags, */
  album_artist?: string;
  disc?: number;
  track?: number;
  year?: number;
  /** its tempo (the BPM tag, else the analysis', decision 0035), */
  bpm?: number;
  /** the user's plays of the track and when it was last played (epoch seconds), */
  play_count?: number;
  last_played?: number;
  /** and when the entry came (none for one held before Cratarr kept that). */
  added?: number;
  /** Its album's id, where the rows are of several albums (an artist's). */
  album_id?: string;
};

/** Take a playlist's repeats out, the first copy of each track staying (decision 0025, amended 2026-10-16). */
export const removeRepeats = (id: string) =>
  request<{ playlist: string } & Undo>(`/api/playlists/${encodeURIComponent(id)}/repeats`, { method: 'DELETE' });

/** Add tracks (ids on `source`, in order) at a playlist's end, whether or not it holds them already: Add again. */
export const appendTracks = (id: string, source: string, ids: string[]) =>
  post<{ playlist: string; added: number } & Undo>(`/api/playlists/${encodeURIComponent(id)}/append`, { source, ids });

export const playlistRows = (id: string, source?: string) =>
  quietly<{ source: string; rows: PlaylistRow[] }>(
    `/api/playlists/${encodeURIComponent(id)}/rows${source ? `?source=${encodeURIComponent(source)}` : ''}`,
  );

/** Saves a playlist edited in its view (decision 0023): its lines reordered or some left out (`entries`), replacing
 *  `expected` (the lines as the view showed them); refused when it changed since. */
export const editPlaylist = (id: string, expected: string[], entries: string[]) =>
  request<{ playlist: string; entries: number } & Undo>(`/api/playlists/${encodeURIComponent(id)}/entries`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ expected, entries }),
  });

export const playlistCovers = (id: string) =>
  quietly<{ source: string; album_id: string }[]>(`/api/playlists/${encodeURIComponent(id)}/covers`);
export const renamePlaylist = (id: string, to: string) =>
  post<{ playlist: string } & Undo>(`/api/playlists/${encodeURIComponent(id)}/rename`, { to });
export const deletePlaylist = (id: string) =>
  request<{ deleted: string } & Undo>(`/api/playlists/${encodeURIComponent(id)}`, { method: 'DELETE' });
/** Shares one's own playlist with the other users, or stops (decision 0051): they see it, play it and copy it. */
export const setPlaylistShared = (id: string, shared: boolean) =>
  request<void>(`/api/playlists/${encodeURIComponent(id)}/shared`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ shared }) });
/** Another user's shared playlist (its id "owner/name"), copied into one's own under a free name: that name. */
export const copyPlaylist = (id: string) => post<{ playlist: string } & Undo>(`/api/playlists/${encodeURIComponent(id)}/copy`, {});

/** The other names a playlist is spoken as, exactly these. */
/** The names a playlist is also spoken as, from what was typed (comma-separated). */
export const spokenNames = (text: string) =>
  text
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);

export const setSpoken = (id: string, names: string[]) =>
  request<Undo>(`/api/playlists/${encodeURIComponent(id)}/spoken`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ names }),
  });
const linkPath = (id: string, target: string, extra: Record<string, string> = {}) =>
  `/api/playlists/${encodeURIComponent(id)}/link?${new URLSearchParams({ target, ...extra })}`;
/** Link a playlist to a target, or change how it is kept (the state wanted: safe to repeat). */
export const linkPlaylist = (id: string, target: string) => request<null>(linkPath(id, target), { method: 'PUT' });
/** Take a link off; its copy on the target is deleted. */
export const unlinkPlaylist = (id: string, target: string) => request<null>(linkPath(id, target), { method: 'DELETE' });

export type Status = {
  connected: boolean;
  /** The server's name with one source; "n of m sources" with several. */
  server_name: string | null;
  /** A Music library is set and its folder found: playlists can be read and made. */
  music: boolean;
  /** Some enabled server has an Incoming library. */
  incoming: boolean;
  sources: SourceTab[];
  version: string;
  /** What is wrong with the servers, one line each (a server that stopped processing changes). */
  problems: string[];
};

export const getStatus = () => quietly<Status>('/api/status');

export type FieldKind = 'url' | 'text' | 'secret' | 'choice' | 'multi_choice';

/** A setting a source plugin asks for; the Settings page draws its form from these. */
export type Field = {
  key: string;
  label: string;
  kind: FieldKind;
  required: boolean;
  help: string | null;
  /** Secrets: changing one of these fields needs the secret entered again. */
  bound_to: string[];
};

export type Choice = { value: string; label: string };

export type SourceStatus = {
  connected: boolean;
  server_name: string | null;
  version: string | null;
  detail: string | null;
  error: string | null;
};

export type Values = Record<string, string | string[]>;

/** What is saved for a source. Secrets left blank keep the saved ones (while their bound fields stay the same). */
/** What a server's libraries are used for, by library id (decision 0009). Libraries in neither list are not used. */
/** A library's own settings (decision 0034): what a move into it does to the files, and its dot's colour. */
export type LibraryPolicy = { format?: 'cd' | 'as_is'; colour?: string | null };
export type Roles = { music: string[]; incoming: string[]; policies?: Record<string, LibraryPolicy> };

export type LibraryUse = 'music' | 'incoming' | 'unused';

/** A server's music library and where Cratarr sees its files. */
export type ServerLibrary = {
  id: string;
  name: string;
  /** Cratarr's folders its files were found in; empty when Cratarr can't see them. */
  folders: string[];
  /** Its folders as the server describes them, to say what to mount. */
  paths: string[];
  /** Cratarr sees its files but cannot write there. */
  read_only: boolean;
};

export type SourceConfig = { id: string; kind: string; enabled: boolean; libraries: Roles; values: Values };

export type SourceView = SourceConfig & {
  label: string;
  /** Secrets are never sent; these are the ones that are set. */
  secrets_set: string[];
  status: SourceStatus;
  choices: Record<string, Choice[]>;
  server_libraries: ServerLibrary[];
  error: string | null;
  /** Each used library's settings as they apply (its own, else its use's and the palette's). */
  policies?: Record<string, { format: 'cd' | 'as_is'; colour: string | null }>;
};

export type Kind = { id: string; label: string; fields: Field[] };

export type Settings = {
  sources: SourceView[];
  kinds: Kind[];
  /** Whether the sources come from the settings file or (first run) the container's environment. */
  origin: 'file' | 'env';
  fixed: { bind: string };
  /** Null when the server has no data folder: settings can be looked at, not saved. */
  settings_file: string | null;
  playlists: number;
  version: string;
  /** Epoch seconds. */
  started: number;
  /** Where the Music libraries' files are, as Cratarr sees them: the playlists live there. */
  music_folder: string | null;
  /** Null when no Lidarr is set. */
  lidarr: LidarrView | null;
  tag_rules: TagRules;
  /** How long albums deleted from Incoming stay in the trash. */
  trash_days: number;
  catalogue: CatalogueStatus | null;
  /** The saved naming template of Music; null: the one the library already follows. */
  naming: string | null;
  /** The libraries' colours to choose from (decision 0034). */
  palette: string[];
};

/** A naming template measured against Music (decision 0013). */
export type NamingView = {
  template: string;
  folders: { matched: number; total: number };
  files: { matched: number; total: number };
  /** An album of the library named by it: folder/first file. */
  example: string | null;
};
/** Measure a template as typed; without one, the template in use. */
export const checkNaming = (template?: string) =>
  quietly<NamingView>(`/api/settings/naming${template === undefined ? '' : `?template=${encodeURIComponent(template)}`}`);

/** How tag values are split when read; the files are never changed. */
export type TagRules = {
  artist_separators: string[];
  genre_separators: string[];
  /** Names containing a separator that are one artist (AC/DC). */
  artist_keep: string[];
  /** Genres containing a separator that are one genre ("Funk / Soul"). */
  genre_keep: string[];
};

export type CatalogueStatus = {
  folders: { path: string; files: number }[];
  files: number;
  tags_pending: number;
  tags_failed: number;
  /** Files with a known id, per server id. */
  linked: Record<string, number>;
  /** Files with a problem, per kind (see FileProblem). */
  issues: Partial<Record<FileProblem['kind'], number>>;
  progress: { doing: string | null; last_pass: number | null; problems: string[] };
};

export const getSettings = () => request<Settings>('/api/settings');

export const getCatalogue = () => quietly<CatalogueStatus>('/api/catalogue');

/** Folders, chosen and shown (decision 0044): a folder in the browser, with how much audio is in it (`more`: past the
 *  count) and whether Cratarr can write there. */
export type FolderEntry = { name: string; path: string; audio: number; more: boolean; writable: boolean };
/** What the browser shows at a place (`path` null: the top, Cratarr's mounts); `parent` null: up is the top. */
export type FolderListing = { path: string | null; parent: string | null; folders: FolderEntry[] };
export const browseFolders = (path?: string | null) =>
  request<FolderListing>(`/api/folders${path ? `?path=${encodeURIComponent(path)}` : ''}`);
/** A chosen folder's row: whether it is there and writable, its disk's free bytes, and the tracks Cratarr knows in it. */
export type FolderStatus = { exists: boolean; writable: boolean; free: number | null; tracks: number | null };
export const folderStatus = (path: string) => quietly<FolderStatus>(`/api/folders/status?path=${encodeURIComponent(path)}`);
/** Music folders Cratarr found that are not chosen yet. */
export const suggestedFolders = () => quietly<{ folders: string[] }>('/api/folders/suggested');

/** A file the catalogue could not read properly. */
export type FileProblem = {
  folder: string;
  rel: string;
  /** broken: cannot be read · damaged_tag: one field unreadable, rest fine · no_tags · no_length */
  kind: 'broken' | 'damaged_tag' | 'no_tags' | 'no_length';
  detail: string | null;
};

export const getFileProblems = () => request<FileProblem[]>('/api/catalogue/problems');

/** The Subsonic accounts (decision 0030): names only, passwords are never sent back. */
export type SubsonicAccounts = { accounts: string[] };
export const getSubsonicAccounts = () => quietly<SubsonicAccounts>('/api/subsonic/accounts');
/** The first account; refused once one exists. */
export const createSubsonicAccount = (username: string, password: string) =>
  post<SubsonicAccounts>('/api/subsonic/accounts', { username, password });
export const changeSubsonicPassword = (username: string, current: string, password: string) =>
  post<SubsonicAccounts>('/api/subsonic/password', { username, current, password });
/** A device signed in on Cratarr's sign-in page (decision 0039); `current`: the one this page runs on. */
export type SignedIn = { id: string; user: string; device: string; created: number; last_used: number; current: boolean };
export const getSignedIn = () => quietly<{ sessions: SignedIn[] }>('/api/auth/sessions');
/** Signs that device out: its next request goes to the sign-in page. */
export const signOutDevice = (id: string) => request<void>(`/api/auth/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });

/** Sign-in everywhere (decision 0043): whether every request needs a sign-in, and switching it (the password again). */
export const getSignInRequired = () => quietly<{ required: boolean }>('/api/auth/required');
export const setSignInRequired = (on: boolean, password: string) =>
  request<{ required: boolean }>('/api/auth/required', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ on, password }) });
/** An API key (decision 0043), for what cannot sign in on a page (Home Assistant, scripts): never the key itself. Since
 *  decision 0051 it acts for its owner, and only reads (`read`) or also plays and changes (`control`). */
export type KeyScope = 'read' | 'control';
export type ApiKey = { id: string; name: string; created: number; last_used: number | null; owner?: string; scope?: KeyScope };
export const getApiKeys = () => quietly<{ keys: ApiKey[] }>('/api/auth/keys');
/** A new key: `key` is shown in this answer only, never again. */
export const newApiKey = (name: string, password: string, owner?: string, scope?: KeyScope) =>
  post<{ key: string; id: string; name: string }>('/api/auth/keys', { name, password, owner, scope });

/** What a user may do beyond listening (decision 0051), in Cratarr and in OpenSubsonic apps alike: keep tracks to play
 *  offline, make playlists, share them with the other users, make public share links. */
export type Permissions = { offline: boolean; playlists: boolean; share: boolean; links: boolean };
/** A device that belongs to an account (it plays only on its own, decision 0051): uid=197609(Oskar) gid=197121 groups=197121 its player's name. One that is
 *  connected, or a Kodi (removed under Players), is not taken away. */
export type OwnDevice = { id: string; name: string; kind: string; online?: boolean; last_seen: number | null };
/** Whether a device can be taken away now. */
export const removable = (d: OwnDevice) => !d.online && d.kind !== 'kodi';
/** One of an account's sign-ins; `current`: the one this page runs on (one's own only). */
export type UserSession = { id: string; device: string; created: number; last_used: number; current?: boolean };
/** Whoever is signed in (decision 0051): their name (from the directory), whether they administer this server, what they
 *  may do, whether they have set a password for OpenSubsonic apps, their devices and sign-ins. */
export type Me = {
  name: string | null;
  admin: boolean;
  /** Deactivated: left the directory's music group (their data kept). */
  active?: boolean;
  /** Signs in through the directory (else Cratarr's own account). */
  directory?: boolean;
  permissions: Permissions;
  app_password?: boolean;
  devices?: OwnDevice[];
  sessions?: UserSession[];
};
export const getMe = () => quietly<Me>('/api/me');
/** Ends one of one's own sign-ins, or takes one of one's own devices away. */
export const endMySession = (id: string) => request<void>(`/api/me/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });
export const removeMyDevice = (id: string) => request<void>(`/api/me/devices/${encodeURIComponent(id)}`, { method: 'DELETE' });
/** The server's version and when it started, for everyone (the rest of its settings are its administrators'). */
export const getAbout = () => quietly<{ version: string; started: number }>('/api/about');
/** One's own settings (decision 0051), one object per account, as the pages and the app name its parts; `version`
 *  counts its saves (0: none kept yet). A save names the version it changed, and is refused (409) if another came first. */
export type MySettings = { version: number; settings: Record<string, unknown> };
export const getMySettings = () => quietly<MySettings>('/api/me/settings');
export const putMySettings = (base: number, settings: Record<string, unknown>) =>
  request<{ version: number }>('/api/me/settings', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ base, settings }) });
/** Each server's own name and libraries, and what each library is used as, for everyone (the filter): the part of the
 *  server's settings that is not its administrators' alone. */
export type LibrariesView = {
  sources: { id: string; status: { server_name: string | null }; server_libraries: { id: string; name: string }[]; libraries: { music: string[]; incoming: string[] } }[];
};
export const getLibraries = () => quietly<LibrariesView>('/api/libraries');
/** Their own password for OpenSubsonic apps, whose sign-in needs the password itself (a directory never gives it). */
export const setAppPassword = (password: string) =>
  request<void>('/api/me/subsonic-password', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ password }) });
/** Every user, for the administrators' Users panel: their profile, permissions, devices, sign-ins. `directory`: an
 *  account from the directory (its name and role set there); else Cratarr's own (decision 0051, amended 2026-10-19). */
export type User = { name: string; admin: boolean; active: boolean; directory?: boolean; permissions: Permissions; devices: OwnDevice[]; sessions: UserSession[] };
export const getUsers = () => request<{ users: User[] }>('/api/users').then((r) => r.users);
const putJson = (url: string, body: unknown) => request<void>(url, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
const userUrl = (name: string, what: string) => `/api/users/${encodeURIComponent(name)}/${what}`;
/** A Cratarr account of its own (the first one there is administers, whatever is asked). */
export const createUser = (name: string, password: string, admin: boolean) => post<User>('/api/users', { name, password, admin });
/** A Cratarr account's role (never one's own, never the last active administrator's). */
export const setUserRole = (name: string, admin: boolean) => putJson(userUrl(name, 'role'), { admin });
/** A Cratarr account's new password, set by an administrator; its other sign-ins end. */
export const setUserPassword = (name: string, password: string) => putJson(userUrl(name, 'password'), { password });
/** A Cratarr account switched off (its data kept, its sign-ins ended) or on again. */
export const setUserActive = (name: string, active: boolean) => putJson(userUrl(name, 'active'), { active });
/** One's own password, for a Cratarr account (it is its OpenSubsonic one too). */
export const changeOwnPassword = (current: string, password: string) => putJson('/api/me/password', { current, password });
export const setPermissions = (name: string, p: Permissions) =>
  request<void>(`/api/users/${encodeURIComponent(name)}/permissions`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(p) });
/** Ends a user's sign-in (an administrator, or the user themselves). */
export const endSession = (name: string, id: string) =>
  request<void>(`/api/users/${encodeURIComponent(name)}/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });
/** Takes a device from a user: it plays for nobody until it signs in again. */
export const removeDevice = (name: string, id: string) =>
  request<void>(`/api/users/${encodeURIComponent(name)}/devices/${encodeURIComponent(id)}`, { method: 'DELETE' });
export const revokeApiKey = (id: string) => request<void>(`/api/auth/keys/${encodeURIComponent(id)}`, { method: 'DELETE' });

/** A directory (LDAP) asked first at sign-in (decision 0040): set by the operator in `ldap.json`, shown read-only. */
export type DirectoryConfig = {
  enabled: boolean;
  url: string;
  starttls: boolean;
  base_dn: string;
  user_attribute: string;
  group: string;
  bind_dn: string;
  bind_password_file: string;
  accounts: Record<string, string>;
};
export type DirectorySettings = { config: DirectoryConfig; bind_password: 'set' | 'not set' | 'unreadable' };
export const getDirectory = () => quietly<DirectorySettings>('/api/auth/ldap');

/** Importing an account's plays, favourites, ratings and added dates from a server into a Subsonic account. */
export type ImportWhat = { plays: boolean; favorites: boolean; ratings: boolean; added: boolean };
/** A server that can import: its accounts (`id` is the server's), and the choices made last time. */
export type ImportSource = {
  id: string;
  users: { id: string; name: string }[];
  user?: string | null;
  account?: string | null;
  what?: ImportWhat;
  error?: string;
};
export type ImportSources = { accounts: string[]; sources: ImportSource[] };
export type ImportRequest = { source: string; user: string; account: string; what: ImportWhat; preview: boolean };
export type ImportCounts = { plays: number; songs: number; starred: number; unstarred: number; rated: number; added: number; skipped: number };
export type ImportJob = ImportRequest & {
  started: number;
  finished: number | null;
  step: { name: 'tracks' | 'groups' | 'added'; read: number; total: number | null } | null;
  counts: ImportCounts;
  error: string | null;
  note: string | null;
};
export const getImportSources = () => quietly<ImportSources>('/api/subsonic/import/sources');
/** The import running now, or the last one. */
export const getImport = () => quietly<{ job: ImportJob | null }>('/api/subsonic/import');
export const startImport = (r: ImportRequest) => post<{ job: ImportJob }>('/api/subsonic/import', r);

/** Lidarr on the Settings page; the key is never sent. */
export type LidarrView = { url: string; key_set: boolean; version: string | null; error: string | null };

/** What the page sends for Lidarr: a blank key keeps the saved one while the address stays; a blank address removes it. */
export type LidarrDraft = { url: string; api_key: string };

/** Check a Lidarr as filled in; nothing is saved. Null for a blank address. */
export const previewLidarr = (draft: LidarrDraft) => post<LidarrView | null>('/api/settings/lidarr', draft);

/** `lidarr` left out: keep the saved one. */
export const saveSettings = (
  sources: SourceConfig[],
  extra: { tag_rules: TagRules; lidarr?: LidarrDraft; trash_days: number; naming?: string },
) =>
  request<Settings>('/api/settings', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ sources, ...extra }),
  });

/** One source as it is being filled in: status, choices and libraries, nothing saved. */
export const previewSource = (source: SourceConfig) => post<SourceView>('/api/settings/preview', { source });

export type AlbumTrack = {
  file: string;
  key: string | null;
  bytes: number;
  seconds: number | null;
  kbps: number | null;
  plays: number;
  favourite: boolean;
};
export type Album = {
  folder: string;
  tracks: AlbumTrack[];
  total_bytes: number;
  cover_bytes: number | null;
  plays: number;
  favourites: number;
  playlist_lines: [string, number][];
};
export type Comparison = {
  same_track_count: boolean;
  matched: number;
  same_length: number;
  same_size: number;
  verdict: 'same' | 'similar' | 'different';
};
export type AlbumGroup = {
  albums: Album[];
  comparisons: Comparison[];
  suggestion: { keep: string; reasons: string[] };
};

export const getAlbumDuplicates = () => request<{ groups: AlbumGroup[] }>('/api/albums/duplicates');

/** An album in a server's library list (Incoming, Collection). */
export type ListedAlbum = {
  id: string;
  title: string;
  artist: string | null;
  year: number | null;
  /** When the source added it, epoch seconds. */
  added: number | null;
  path: string | null;
  tracks: number;
  /** e.g. "FLAC", "MP3"; null when mixed or unknown. */
  format: string | null;
  /** The lowest track bitrate. */
  bitrate_kbps: number | null;
  /** The source's own tags, as they are. */
  tags: string[];
  /** The library it was listed from (the filter's library switches). */
  library?: string;
  /** Its artists one by one, when the source credits several; absent: `artist` alone. */
  artists?: string[];
};

/** One server's album list. */
export type ShelfSource = {
  id: string;
  label: string;
  /** False when this kind of source cannot list albums at all. */
  supported: boolean;
  albums: ListedAlbum[];
  error: string | null;
  /** When the server read this list from the source, epoch seconds. */
  as_of: number;
  /** The libraries it was read from (the filter's library switches). */
  libraries?: string[];
  /** Those libraries' names, uses, colours and formats (the Library's list, decision 0034). */
  library_info?: LibraryInfo[];
};

/** A used library of a server (decision 0034): the dot on its albums (none for the primary, Music), and what a move
 *  into it does to the files. */
export type LibraryInfo = { id: string; name: string; use: 'music' | 'incoming'; colour: string | null; format: 'cd' | 'as_is' };

/** An album of a library other than Music, as it stands to Music: an upgrade of an album there, or a duplicate. */
export type Verdict = { kind: 'upgrade' | 'duplicate'; library: string; why: string };

/** A server's albums of every used library in one list (decision 0034), with what holds between them. */
export type AlbumsSource = ShelfSource & {
  library_info: LibraryInfo[];
  verdicts: Record<string, Verdict>;
  /** Per Music album: an album of another library that upgrades or duplicates it. */
  waiting: Record<string, { kind: 'upgrade' | 'duplicate'; album: string }>;
  deletable: string[];
};
export type AlbumsView = { sources: AlbumsSource[]; trash_days: number; music: string | null };

/** Every album of each server's used libraries (the Library page). `fresh` asks past the service worker's copy (which
 *  answers at once, see sw.js) for the server's latest. */
export const getAlbums = (source?: string, fresh = false) => quietly<AlbumsView>(`/api/albums${follow(source)}`, fresh ? { cache: 'no-cache' } : undefined);
/** Play an album of a connected server on the followed player; answers with the player's label. */
export const playAlbum = (source: string, album: string) =>
  request<{ player: string }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/play`, { method: 'POST' });
/** Make an album a favourite or not (the album panel's heart). */
export const setAlbumFavorite = (source: string, album: string, on: boolean) =>
  request<{ favorite: boolean }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/favorite?on=${on}`, { method: 'POST' });
/** The album's cover as the server has it (no picture chosen on this device, no reading again). */
export const serverCoverUrl = (source: string, album: string) => `/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/cover`;
/** The album's cover as this device shows it: a picture chosen here, else the server's (read again: a new address). */
export const albumCoverUrl = (source: string, album: string) => {
  const key = coverKey(source, album);
  const round = coverRounds[key];
  return localCovers[key] ?? `${serverCoverUrl(source, album)}${round ? `?v=${round}` : ''}`;
};
/** How the owner used an album (decided 2026-10-16: the Library's orders and filters): its plays, when last played
 *  (epoch seconds), its rating (0: none). */
export type AlbumUse = { plays: number; played?: number; rating: number };
/** Every album of a server played or rated, by id; read fresh each time (plays change). */
export const getAlbumUses = (source: string) => quietly<Record<string, AlbumUse>>(`/api/album-uses/${encodeURIComponent(source)}`);
/** An album's rating, 0 to 5; null when its server keeps none. */
export const getAlbumRating = (source: string, album: string) =>
  quietly<{ rating: number | null }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/rating`);
export const setAlbumRating = (source: string, album: string, stars: number) =>
  request<{ rating: number }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/rating?stars=${stars}`, { method: 'POST' });

export type Transport = 'previous' | 'play-pause' | 'next' | 'stop';

// The local player (decision 0022): a playlist's track ids, a track's details, and handing a queue to the server's
// player (from a track and a position).
export const playlistTrackIds = (id: string, source?: string) =>
  request<{ source: string; ids: string[]; unknown: number }>(`/api/playlists/${encodeURIComponent(id)}/tracks${follow(source)}`);
const trackPath = (source: string, id: string) => `/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}`;

export const trackDetails = (source: string, id: string) =>
  quietly<{ id: string; name: string; artist: string | null; album: string | null; album_id: string | null }>(trackPath(source, id));

/** A given track's playlists and favourite: for the local player's track (decision 0022), what `State` is for the
 *  server's player's. `favorite` null: not known. */
export type TrackState = { addable: boolean; album_id: string | null; favorite: boolean | null; playlists: Playlist[]; info: TrackInfo | null };

export const trackState = (source: string, id: string) => quietly<TrackState>(`${trackPath(source, id)}/playlists`);

/** A track's file as it is (the track menu's Details): where it lies (`path` within `folder`), its audio, its size,
 *  when it was changed and came in (epoch s), and every tag in the file's order. */
export type TrackFile = {
  path: string;
  folder: string | null;
  format: string | null;
  bitrate_kbps: number | null;
  sample_rate_hz: number | null;
  bit_depth: number | null;
  channels: number | null;
  duration_ms: number | null;
  /** Its tempo (decision 0035): the BPM tag, else the analysis'; none without a beat. */
  bpm?: number | null;
  size: number | null;
  modified: number | null;
  added: number | null;
  /** Its genres as a genre in the panel matches them. */
  genres: string[];
  tags: { name: string; value: string }[];
};
export const trackFile = (source: string, id: string) => quietly<TrackFile>(`${trackPath(source, id)}/details`);

/** Put the track in the playlist or take it out (the state wanted, as `setMember`). */
export const setTrackMember = (source: string, id: string, playlistId: string, member: boolean) =>
  quietly<TrackState>(`${trackPath(source, id)}/playlists/${encodeURIComponent(playlistId)}?member=${member}`, { method: 'POST' });

export const setTrackFavorite = (source: string, id: string, on: boolean) =>
  quietly<TrackState>(`${trackPath(source, id)}/favorite?on=${on}`, { method: 'POST' });

/** Which of these tracks (ids on `source`) are favourites, in one request: the hearts on a list's rows (decision 0023).
 *  Tracks the server cannot tell about are left out. */
export const getFavorites = (source: string, ids: string[]) =>
  quietly<Record<string, boolean>>(`/api/favorites/${encodeURIComponent(source)}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids }),
  });

/** The favourite albums of a server (their ids): the Library's Favourites only and the albums' hearts. */
export const getFavoriteAlbums = (source: string) => quietly<string[]>(`/api/favorites/${encodeURIComponent(source)}/albums`);

/** Which playlists (by name) are favourites, as Finamp's playlist hearts (decision 0023). */
export const getPlaylistFavorites = (source?: string) => quietly<Record<string, boolean>>(`/api/playlists/favorites${follow(source)}`);

export const setPlaylistFavorite = (id: string, on: boolean, source?: string) => {
  const q = new URLSearchParams({ on: String(on) });
  if (source) q.set('source', source);
  return quietly<{ playlist: string; favorite: boolean }>(`/api/playlists/${encodeURIComponent(id)}/favorite?${q}`, { method: 'POST' });
};

/** The local player played most of the track: one play counts in the media server (decision 0022). */
export const trackPlayed = (source: string, id: string) => quietly<{ counted: boolean }>(`${trackPath(source, id)}/played`, { method: 'POST' });
/** An album as its view in the panel shows it: its tracks in play order, as a playlist's rows (decision 0023). */
/** `id`: the album's, another than asked for when it moved (Keep, Replace) and the server followed it. */
export type AlbumView = {
  source: string;
  id: string;
  title: string;
  artist: string | null;
  year: number | null;
  genres: string[];
  /** "FLAC", "MP3"…; null when mixed or unknown. */
  format: string | null;
  /** Its weakest track's bitrate. */
  bitrate_kbps: number | null;
  rows: PlaylistRow[];
  /** When it came and when it was last played (epoch seconds), and how often (the panel's About); absent where the
   *  server cannot say. */
  added?: number;
  played?: number;
  plays?: number;
};
export const albumView = (source: string, album: string) =>
  quietly<AlbumView>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}`);
/** An artist as their panel shows them (decided 2026-10-16, as Symfonium's), by name as the albums credit them: their
 *  albums and the albums they appear on, newest first, and their tracks album by album with the plays. */
export type ArtistView = {
  source: string;
  name: string;
  albums: ListedAlbum[];
  appears_on: ListedAlbum[];
  rows: PlaylistRow[];
  /** Null where the server keeps no favourite artists. */
  favorite?: boolean;
};
export const artistView = (source: string, name: string) =>
  quietly<ArtistView>(`/api/artists/${encodeURIComponent(source)}?name=${encodeURIComponent(name)}`);
/** Whether an artist is a favourite (the heart on an album's artist chip); null where the server keeps none. */
export const artistFavorite = (source: string, name: string) =>
  quietly<{ favorite: boolean | null }>(`/api/artists/${encodeURIComponent(source)}/favorite?name=${encodeURIComponent(name)}`);
export const setArtistFavorite = (source: string, name: string, on: boolean) =>
  request<{ favorite: boolean }>(`/api/artists/${encodeURIComponent(source)}/favorite?name=${encodeURIComponent(name)}&on=${on}`, { method: 'POST' });
/** Where an album of Cratarr's own library was left (Resume): a track and a position, from the bookmarks apps keep
 *  (decision 0030); `track` null when there is none. */
export const albumResume = (album: string) =>
  quietly<{ track: string | null; position_secs?: number }>(`/api/library/albums/${encodeURIComponent(album)}/resume`);
/** The album's resume point gone. */
export const forgetAlbumResume = (album: string) => request<void>(`/api/library/albums/${encodeURIComponent(album)}/resume`, { method: 'DELETE' });
export const handover = (ids: string[], index: number, positionSecs: number, source?: string) =>
  post<{ player: string }>(`/api/player/handover${follow(source)}`, { ids, index, position_secs: positionSecs });
/** Previous / play-pause / next on the player that plays now. */
export const transport = (command: Transport, source?: string) =>
  quietly<{ player: string }>(`/api/player/${command}${follow(source)}`, { method: 'POST' }); // shown at once

/** What a move would do with an album: its new folder in the library, or why not (`needs_year`: ask for the year). */
export type KeepPreview = {
  title: string;
  folder: string | null;
  files: number;
  refused: { kind: 'needs_year' } | { kind: 'other'; detail: string } | null;
  /** Hi-res files converted to 16-bit/44.1 kHz first (the move then waits in the queue). */
  convert: number;
  /** Whether a Lidarr is set to be told to stop fetching it; without one it may fetch the album again. */
  acquisition: boolean;
};
/** What a move or Replace did: done (`undo`), or queued until its hi-res files are converted (`queued`). */
export type AlbumMoved = { undo: string | null; title: string; folder: string; queued: boolean };
const albumPath = (source: string, album: string) => `/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}`;
/** What moving an album into library `to` would do (decision 0034): its folder there, or why it can't. */
export const movePreview = (source: string, album: string, to: string, year?: string) =>
  request<KeepPreview>(`${albumPath(source, album)}/move?to=${encodeURIComponent(to)}${year ? `&year=${encodeURIComponent(year)}` : ''}`);
/** Move an album into another used library of its server, named and converted that library's way (Undo brings it
 *  back); into a library of CD format with hi-res files it is queued. */
export const moveAlbum = (source: string, album: string, to: string, year?: string) =>
  request<AlbumMoved>(`${albumPath(source, album)}/move`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(year ? { to, year } : { to }),
  });
/** Delete an album of any library, into that library's trash (Undo brings it back). */
export const deleteAlbum = (source: string, album: string) => request<{ undo: string; title: string }>(`${albumPath(source, album)}/delete`, { method: 'POST' });

/** What Replace would do with an upgrade: the album in Music it replaces, how many of its tracks the new files take
 * over, new tracks, and where it goes; or why not (`unmatched`: old tracks without a match, by title). */
export type ReplacePreview = {
  title: string;
  old: string;
  folder: string | null;
  tracks: number;
  added: number;
  refused: { kind: 'needs_year' } | { kind: 'unmatched'; detail: string[] } | { kind: 'other'; detail: string } | null;
  convert: number;
  /** Whether a Lidarr is set to be told to stop fetching it; without one it may fetch the album again. */
  acquisition: boolean;
};
const replacePath = (source: string, album: string) =>
  `/api/incoming/replace/${encodeURIComponent(source)}/${encodeURIComponent(album)}`;
export const replacePreview = (source: string, album: string, year?: string) =>
  request<ReplacePreview>(`${replacePath(source, album)}${year ? `?year=${encodeURIComponent(year)}` : ''}`);
/** Replace the album in Music with an upgrade: the old files go to the trash (Undo brings them back). */
export const replaceIncoming = (source: string, album: string, year?: string) =>
  request<AlbumMoved>(replacePath(source, album), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(year ? { year } : {}),
  });

/** An album in the conversion queue: waiting, converting (`done` of `total` files), or just finished. */
export type Job = {
  id: number;
  source: string;
  album_id: string;
  title: string;
  action: 'keep' | 'replace';
  state: 'waiting' | 'converting' | 'done' | 'failed';
  done: number;
  /** How far the file being converted is, 0 to 1. */
  part: number;
  total: number;
  folder: string;
  undo: string | null;
  why: string | null;
};
export const getJobs = () => quietly<Job[]>('/api/incoming/jobs');
/** Take an album out of the queue while it waits or converts. */
export const cancelJob = (id: number) => request<null>(`/api/incoming/jobs/${id}/cancel`, { method: 'POST' });

/** A name with a count: "Downtempo", 120 albums. */
export type Count = { name: string; n: number };

export type Insights = {
  library: {
    tracks: number;
    albums: number;
    artists: number;
    duration_ms: number;
    bytes: number;
    /** Tracks per format. */
    formats: Count[];
    lossless_albums: number;
    below_320_albums: number;
    /** Albums per decade, oldest first ("1970s"). */
    decades: Count[];
    /** Albums per genre / album artist, most first. */
    genres: Count[];
    top_artists: Count[];
  };
  playlists: {
    playlists: number;
    distinct_tracks: number;
    in_one_playlist: number;
    missing: number;
    library_tracks_in_playlists: number;
    albums_without_playlist: number;
    most_shared: { title: string; artist: string | null; playlists: number }[];
  };
  proximity: Proximity;
};

/** Playlists as points and how close each two are, by three signals (0..1 each). */
export type Proximity = {
  /** `focus`: how concentrated its genres are, 0..1 (null without genre tags). */
  playlists: { name: string; tracks: number; genres: string[]; focus: number | null }[];
  links: {
    a: number;
    b: number;
    tracks: number;
    genre: number;
    artists: number;
    shared_tracks: number;
    shared_artists: number;
    genres: string[];
  }[];
};

export type Signal = 'tracks' | 'genre' | 'artists';

export const getInsights = () => request<Insights>('/api/insights');

/** A sync group's visuals (decision 0046, amended): the settings that shape the picture, and the version the server
 *  gave them. None yet: a 404. */
export type GroupVisuals = { version: number; settings: Record<string, unknown> };
export const getGroupVisuals = (group: string) => quietly<GroupVisuals>(`/api/groups/${encodeURIComponent(group)}/visuals`);
/** Settings into a group's visuals (the server merges them key by key, the routes route by route): `base` 0 only where
 *  the group has none yet (else a 409); `writer` names this page, so it knows its own change when it comes back. */
export const putGroupVisuals = (group: string, body: { base: number; settings: Record<string, unknown>; writer: string }) =>
  quietly<{ version: number }>(`/api/groups/${encodeURIComponent(group)}/visuals`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

/** What the visuals' governor learned from the owner's thumbs (learning.ts): each limit's value, the step its next
 *  change takes and which way it last went, as of `at` (ms); per user. `version` 0: nothing learned yet. */
export type VisualsLimits = { version: number; limits: Record<string, { value: number; step: number; dir: -1 | 0 | 1 }>; at: number };
export const getVisualsLimits = () => quietly<VisualsLimits>('/api/visuals/limits');
/** What was learned, over the version `base` (0 where there is none yet; another a 409: load and apply it again). */
export const putVisualsLimits = (body: { base: number; limits: VisualsLimits['limits']; at: number }) =>
  quietly<{ version: number }>('/api/visuals/limits', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
/** What was learned, forgotten: the limits where they started. */
export const forgetVisualsLimits = () => quietly<void>('/api/visuals/limits', { method: 'DELETE' });
/** A thumb, with what the visuals did over the 20 seconds before it and what it changed, for the periodic fit. */
export const postVisualsThumb = (body: Record<string, unknown>) =>
  quietly<void>('/api/visuals/thumbs', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

/** A saved look of the visuals: the settings that shape the picture, named (after the track it was saved on). */
export type VisualsSnapshot = {
  id: string;
  name: string;
  settings: Record<string, unknown>;
  /** Epoch seconds. */
  created: number;
  /** A small picture of the look (a JPEG data URL); none for the first ones saved. */
  picture?: string | null;
};
export const getVisualsSnapshots = () => quietly<VisualsSnapshot[]>('/api/visuals/snapshots');
export const saveVisualsSnapshot = (name: string, settings: Record<string, unknown>, picture: string | null = null) =>
  request<VisualsSnapshot>('/api/visuals/snapshots', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(picture ? { name, settings, picture } : { name, settings }),
  });
export const renameVisualsSnapshot = (id: string, name: string) =>
  request<VisualsSnapshot>(`/api/visuals/snapshots/${encodeURIComponent(id)}`, { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name }) });
/** A snapshot saved over: the visuals' settings now and a new picture, its name (changed, where given) and time kept. */
export const saveOverVisualsSnapshot = (id: string, settings: Record<string, unknown>, picture: string | null, name?: string) =>
  request<VisualsSnapshot>(`/api/visuals/snapshots/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ settings, ...(picture ? { picture } : {}), ...(name ? { name } : {}) }),
  });
export const deleteVisualsSnapshot = (id: string) => request<void>(`/api/visuals/snapshots/${encodeURIComponent(id)}`, { method: 'DELETE' });
