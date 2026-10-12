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
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  /** False when the track is outside the curated library, so it cannot be added to a playlist. */
  addable: boolean;
  /** Null when nothing reports a position. */
  progress: Progress | null;
};

export type Progress = {
  position_secs: number;
  duration_secs: number | null;
  paused: boolean;
  /** No player reports this track; the position is time since it started (wrong after a pause or seek). */
  estimated: boolean;
};

export type State = {
  now_playing: NowPlaying | null;
  playlists: Playlist[];
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

export const getState = () => request<State>('/api/state');

export const toggle = (playlistId: string) =>
  request<State>(`/api/playlists/${encodeURIComponent(playlistId)}/toggle`, { method: 'POST' });

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
