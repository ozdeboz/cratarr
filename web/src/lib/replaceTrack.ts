// Single-track replacement (0.9.8, a5's routes; the owner: an album missing a track gets it one at a time when the whole
// album cannot be had): the album's release as MusicBrainz has it (which tracks are there), one Soulseek search for a
// track, its candidates, one fetched and checked (decoded whole, its length, AcoustID naming that recording), then added
// to the album's folder, tagged from MusicBrainz, with Undo. Administrators only.

export type ReleaseTrack = {
  disc: number;
  position: number;
  title: string;
  artist: string | null;
  lengthMs: number | null;
  recording: string;
  have: { source: string; id: string } | null;
};
export type Release = { release: string; title: string; artist: string | null; year: number | null; tracks: ReleaseTrack[] };
export type Candidate = {
  id: string;
  user: string;
  file: string;
  format: string;
  lossless: boolean;
  bitrate: number | null;
  sampleRate: number | null;
  bitDepth: number | null;
  size: number;
  lengthMs: number | null;
  freeSlot: boolean;
  queue: number | null;
  speed: number | null;
};
export type Search = { state: 'searching' | 'done' | 'failed'; candidates: Candidate[]; bandcamp: string | null; why?: string };
export type Fetch = { state: 'queued' | 'downloading' | 'checking' | 'passed' | 'refused' | 'added'; percent: number | null; why?: string; score?: number };

async function call<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
  const r = await fetch(path, body === undefined ? { method } : { method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  if (!r.ok) {
    let why = '';
    try {
      why = (await r.json()).error ?? '';
    } catch {
      /* no words */
    }
    throw new Error(why || (r.status === 404 ? 'This Cratarr cannot do that yet.' : `Cratarr said ${r.status}.`));
  }
  return r.status === 204 ? (undefined as T) : r.json();
}
const enc = encodeURIComponent;

export const getRelease = (source: string, album: string) => call<Release>(`/api/albums/${enc(source)}/${enc(album)}/release`);
export const searchTrack = (album: { source: string; id: string }, recording: string, replace?: { source: string; id: string }) =>
  call<{ id: string }>('/api/track-search', 'POST', { album, recording, ...(replace ? { replace } : {}) });
export const getSearch = (id: string) => call<Search>(`/api/track-search/${enc(id)}`);
export const fetchCandidate = (search: string, candidate: string) => call<{ id: string }>(`/api/track-search/${enc(search)}/fetch`, 'POST', { candidate });
export const getFetch = (id: string) => call<Fetch>(`/api/track-fetch/${enc(id)}`);
export const dropFetch = (id: string) => call<void>(`/api/track-fetch/${enc(id)}`, 'DELETE');
export const addFetched = (id: string) => call<{ undo: string[] }>(`/api/track-fetch/${enc(id)}/add`, 'POST', {});
