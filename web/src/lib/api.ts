export type Playlist = {
  id: string;
  name: string;
  member: boolean;
  /** Tracks that resolve to a file (what players show). */
  count: number;
  /** Entries that point at a missing file. */
  dead: number;
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

export class ApiError extends Error {}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    throw new ApiError('Cannot reach the Cratarr server');
  }
  if (!res.ok) {
    let message = `Server returned ${res.status}`;
    try {
      const body = await res.json();
      if (typeof body.error === 'string') message = body.error;
    } catch {
      /* keep the generic message */
    }
    throw new ApiError(message);
  }
  return res.json() as Promise<T>;
}

/** `source`: follow that source; absent: whichever started playing most recently. */
const follow = (source?: string) => (source ? `?source=${encodeURIComponent(source)}` : '');

export const getState = (source?: string) => request<State>(`/api/state${follow(source)}`);

export const coverUrl = (np: NowPlaying) => `/api/cover/${encodeURIComponent(np.source)}/${encodeURIComponent(np.id)}`;

/** play: in file order · shuffle: random order · next: after the current track · last: at the end of the queue. */
export type PlayMode = 'play' | 'shuffle' | 'next' | 'last';
/** sent ≤ 240 of total playable tracks; unknown = lines left out (missing, or not known to the server yet). */
export type PlayResult = { player: string; sent: number; total: number; unknown: number };

export const playPlaylist = (playlistId: string, mode: PlayMode, source?: string) =>
  post<PlayResult>(`/api/playlists/${encodeURIComponent(playlistId)}/play${follow(source)}`, { mode });

export const toggle = (playlistId: string, source?: string) =>
  request<State>(`/api/playlists/${encodeURIComponent(playlistId)}/toggle${follow(source)}`, { method: 'POST' });

export type Repair = { playlist: string; from: string; to: string };
export type Repairs = { safe: Repair[]; review: Repair[] };

export const getRepairs = () => request<Repairs>('/api/repairs');

const post = <T>(path: string, body: unknown) =>
  request<T>(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

/** Apply every safe repair. */
export const applySafe = () => post<{ playlists: string[] }>('/api/repairs/apply', { kind: 'safe' });

/** Apply one offered repair (used for the ones that need a look). */
export const applyOne = (r: Repair) => post<{ playlists: string[] }>('/api/repairs/apply', { kind: 'one', ...r });

/** Put playlists back as they were before their last change. */
export const undo = (playlists: string[]) => post<{ restored: number }>('/api/undo', { playlists });

export type DupGroup = { playlist: string; extra: number; entries: { entry: string; copies: number }[] };

export const getDuplicates = () => request<DupGroup[]>('/api/duplicates');
export const dedupe = (playlist: string) => post<{ removed: number }>('/api/duplicates/dedupe', { playlist });

export const createPlaylist = (name: string) => post<{ playlist: string }>('/api/playlists', { name });
export const renamePlaylist = (id: string, to: string) =>
  post<{ playlist: string }>(`/api/playlists/${encodeURIComponent(id)}/rename`, { to });
export const deletePlaylist = (id: string) =>
  request<{ deleted: string }>(`/api/playlists/${encodeURIComponent(id)}`, { method: 'DELETE' });
export const restorePlaylist = (id: string) =>
  post<{ restored: string }>(`/api/playlists/${encodeURIComponent(id)}/restore`, {});

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
};

export const getStatus = () => request<Status>('/api/status');

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
};

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

export const getCatalogue = () => request<CatalogueStatus>('/api/catalogue');

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
export const saveSettings = (sources: SourceConfig[], extra: { tag_rules: TagRules; lidarr?: LidarrDraft; trash_days: number }) =>
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

/** An album in a source's incoming library. */
export type IncomingAlbum = {
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

export type IncomingSource = {
  id: string;
  label: string;
  /** False when this kind of source cannot list albums at all. */
  supported: boolean;
  albums: IncomingAlbum[];
  error: string | null;
  /** When the server read this list from the source, epoch seconds. */
  as_of: number;
  /** Per album id, when it is already in Music: an upgrade of it, or a duplicate. New albums are absent. */
  verdicts: Record<string, { kind: 'upgrade' | 'duplicate'; library: string; why: string }>;
  /** The albums Cratarr can delete (it sees their folder and can write there). */
  deletable: string[];
};

/** `fresh` asks past the service worker's copy (which answers at once, see sw.js) for the server's latest. */
export const getIncoming = (source?: string, fresh = false) =>
  request<{ sources: IncomingSource[]; trash_days: number }>(`/api/incoming${follow(source)}`, fresh ? { cache: 'no-cache' } : undefined);
/** Play an incoming album on the followed player; answers with the player's label. */
export const playIncoming = (source: string, album: string) =>
  request<{ player: string }>(`/api/incoming/play/${encodeURIComponent(source)}/${encodeURIComponent(album)}`, { method: 'POST' });
export const incomingCoverUrl = (source: string, album: string) =>
  `/api/incoming/cover/${encodeURIComponent(source)}/${encodeURIComponent(album)}`;

export type Transport = 'previous' | 'play-pause' | 'next';
/** Previous / play-pause / next on the player that plays now. */
export const transport = (command: Transport, source?: string) =>
  request<{ player: string }>(`/api/player/${command}${follow(source)}`, { method: 'POST' });

/** Delete an Incoming album: it goes to the trash in its folder (Undo brings it back; emptied after the days set in Settings). */
export const deleteIncoming = (source: string, album: string) =>
  request<{ undo: string; title: string }>(`/api/incoming/delete/${encodeURIComponent(source)}/${encodeURIComponent(album)}`, {
    method: 'POST',
  });
export const undoDelete = (undo: string) =>
  request<{ restored: string }>('/api/incoming/undo', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ undo }),
  });
