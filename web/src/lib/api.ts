import { track } from './progress.svelte';
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
};

/** Which side leads when both changed. */
export type Conflict = 'newest' | 'playlist' | 'target';
/** A sync link: a playlist kept in step on a target. */
export type Link = {
  /** "shared", or "user/<server>/<account>". */
  target: string;
  label: string;
  conflict: Conflict;
  status: { at: number; error: string | null } | null;
};
/** A target a playlist can be linked to. `copy_only`: a copy the server cannot edit (Shared), so nothing comes back
 *  from it and it has no conflict rule. */
export type TargetChoice = { target: string; label: string; copy_only: boolean };

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

/** `generic`: no words from the server, only its status; callers say what failed instead (see toast.fail). */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly generic = false,
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
  return fetchJson<T>(path, init);
}

async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    throw new ApiError('Cannot reach the Cratarr server');
  }
  if (!res.ok) {
    let message: string | null = null;
    try {
      const body = await res.json();
      if (typeof body.error === 'string') message = body.error;
    } catch {
      /* no words from the server */
    }
    throw message ? new ApiError(message) : new ApiError(`Server returned ${res.status}`, true);
  }
  return res.json() as Promise<T>;
}

/** `source`: follow that source; absent: whichever started playing most recently. */
const follow = (source?: string) => (source ? `?source=${encodeURIComponent(source)}` : '');

export const getState = (source?: string) => quietly<State>(`/api/state${follow(source)}`);

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
/** The crate panel's favourites toggle (decision 0024, amended): all its tracks, only the favourites, or none. */
export type CrateFavourites = 'all' | 'only' | 'none';
/** `favourites`: the toggle's setting as the page shows it (a tap may be on its way); the saved one without. */
export const crateRows = (id: string, offset: number, limit: number, source?: string, favourites?: CrateFavourites) => {
  const q = new URLSearchParams({ offset: String(offset), limit: String(limit) });
  if (favourites) q.set('favourites', favourites);
  if (source) q.set('source', source);
  return quietly<{ source: string; total: number; rows: PlaylistRow[]; genres: string[] }>(`/api/crates/${encodeURIComponent(id)}/rows?${q}`);
};
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
/** `ids`: the queue to make it from (the local player's); without, the server's player's queue. */
export const playlistFromQueue = (name: string, source?: string, ids?: string[]) =>
  post<{ playlist: string; added: number; left_out: number } & Undo>(`/api/queue/playlist${follow(source)}`, ids ? { name, ids } : { name });

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

/** Apply every safe repair, or only those of these playlists (the panel's Repair tab, decision 0025). */
export const applySafe = (playlists?: string[]) =>
  post<{ playlists: string[] } & Undo>('/api/repairs/apply', playlists ? { kind: 'safe', playlists } : { kind: 'safe' });

/** Apply one offered repair (used for the ones that need a look). */
export const applyOne = (r: Repair) => post<{ playlists: string[] } & Undo>('/api/repairs/apply', { kind: 'one', ...r });

/** The one undo (decision 0026): an action's tokens, undone newest first; the redo's tokens come back (none for
 *  Incoming's, whose albums' titles come back instead). */
export const undo = (tokens: string[]) => post<{ redo: string[]; restored: string[] }>('/api/undo', { tokens });

export type DupGroup = { playlist: string; extra: number; entries: { entry: string; copies: number }[] };

export const getDuplicates = () => request<DupGroup[]>('/api/duplicates');
export const dedupe = (playlist: string) => post<{ removed: number } & Undo>('/api/duplicates/dedupe', { playlist });

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
};

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

export const getTargets = () => request<TargetChoice[]>('/api/sync/targets');
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
export const linkPlaylist = (id: string, target: string, conflict: Conflict) =>
  request<null>(linkPath(id, target, { conflict }), { method: 'PUT' });
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
export type Roles = { music: string[]; incoming: string[] };

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

/** A file the catalogue could not read properly. */
export type FileProblem = {
  folder: string;
  rel: string;
  /** broken: cannot be read · damaged_tag: one field unreadable, rest fine · no_tags · no_length */
  kind: 'broken' | 'damaged_tag' | 'no_tags' | 'no_length';
  detail: string | null;
};

export const getFileProblems = () => request<FileProblem[]>('/api/catalogue/problems');

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
};

export type IncomingSource = ShelfSource & {
  /** Per album id, when it is already in Music: an upgrade of it, or a duplicate. New albums are absent. */
  verdicts: Record<string, { kind: 'upgrade' | 'duplicate'; library: string; why: string }>;
  /** The albums Cratarr can delete (it sees their folder and can write there). */
  deletable: string[];
};

/** `fresh` asks past the service worker's copy (which answers at once, see sw.js) for the server's latest. */
export const getIncoming = (source?: string, fresh = false) =>
  quietly<{ sources: IncomingSource[]; trash_days: number; music: string | null }>(`/api/incoming${follow(source)}`, fresh ? { cache: 'no-cache' } : undefined);
/** The albums of each server's Music library, newest first. */
export const getCollection = (source?: string, fresh = false) =>
  quietly<{ sources: ShelfSource[] }>(`/api/collection${follow(source)}`, fresh ? { cache: 'no-cache' } : undefined);
/** Play an album of a connected server on the followed player; answers with the player's label. */
export const playAlbum = (source: string, album: string) =>
  request<{ player: string }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/play`, { method: 'POST' });
/** Make an album a favourite or not (the album panel's heart). */
export const setAlbumFavorite = (source: string, album: string, on: boolean) =>
  request<{ favorite: boolean }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/favorite?on=${on}`, { method: 'POST' });
export const albumCoverUrl = (source: string, album: string) =>
  `/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/cover`;

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
export type AlbumView = { source: string; id: string; title: string; artist: string | null; year: number | null; genres: string[]; rows: PlaylistRow[] };
export const albumView = (source: string, album: string) =>
  quietly<AlbumView>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}`);
export const handover = (ids: string[], index: number, positionSecs: number, source?: string) =>
  post<{ player: string }>(`/api/player/handover${follow(source)}`, { ids, index, position_secs: positionSecs });
/** Previous / play-pause / next on the player that plays now. */
export const transport = (command: Transport, source?: string) =>
  quietly<{ player: string }>(`/api/player/${command}${follow(source)}`, { method: 'POST' }); // shown at once

/** Delete an Incoming album: it goes to the trash in its folder (Undo brings it back; emptied after the days set in Settings). */
export const deleteIncoming = (source: string, album: string) =>
  request<{ undo: string; title: string }>(`/api/incoming/delete/${encodeURIComponent(source)}/${encodeURIComponent(album)}`, {
    method: 'POST',
  });

/** What Keep would do with an album: its new folder in Music, or why not (`needs_year`: ask for the year). */
export type KeepPreview = {
  title: string;
  folder: string | null;
  files: number;
  refused: { kind: 'needs_year' } | { kind: 'other'; detail: string } | null;
  /** Hi-res files converted to 16-bit/44.1 kHz first (Keep then waits in the queue). */
  convert: number;
  /** Whether a Lidarr is set to be told to stop fetching it; without one it may fetch the album again. */
  acquisition: boolean;
};
/** What Keep or Replace did: done (`undo`), or queued until its hi-res files are converted (`queued`). */
export type AlbumMoved = { undo: string | null; title: string; folder: string; queued: boolean };
const keepPath = (source: string, album: string) => `/api/incoming/keep/${encodeURIComponent(source)}/${encodeURIComponent(album)}`;
export const keepPreview = (source: string, album: string, year?: string) =>
  request<KeepPreview>(`${keepPath(source, album)}${year ? `?year=${encodeURIComponent(year)}` : ''}`);
/** Keep an Incoming album: it moves into Music, named the library's way (Undo brings it back). */
export const keepIncoming = (source: string, album: string, year?: string) =>
  request<AlbumMoved>(keepPath(source, album), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(year ? { year } : {}),
  });

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
