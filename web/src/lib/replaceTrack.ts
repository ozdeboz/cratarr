// Single-track replacement (0.9.8, the routes; the owner: an album missing a track gets it one at a time when the whole
// album cannot be had): the album's release as MusicBrainz has it (which tracks are there), one Soulseek search for a
// track, its candidates, one fetched and checked (decoded whole, its length, AcoustID naming that recording), then added
// to the album's folder, tagged from MusicBrainz, with Undo. Administrators only.
import type { FileRun } from './api';

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
/** A copy as its row says it: its format (lossless with its depth and rate), size, slot and who has it. */
export const copyLine = (c: Candidate) =>
  [
    c.lossless ? [c.format, c.bitDepth && c.sampleRate ? `${c.bitDepth}/${Math.round(c.sampleRate / 100) / 10}` : null].filter(Boolean).join(' ') : `${c.format} ${c.bitrate ?? ''} kbps`,
    `${(c.size / 1048576).toFixed(c.size < 10485760 ? 1 : 0)} MB`,
    c.freeSlot ? 'free slot' : c.queue !== null ? `${c.queue} waiting` : null,
    c.user,
  ]
    .filter(Boolean)
    .join(' · ');
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
/** A release read once and kept (Health's Work through reads the next album's ahead, and an album opened in Health's
 *  panel reads its missing tracks): forgotten when a track is added to the album, or the answer failed. */
const releases = new Map<string, Promise<Release>>();
export function releaseOf(source: string, album: string): Promise<Release> {
  const key = `${source}/${album}`;
  let p = releases.get(key);
  if (!p) {
    p = getRelease(source, album);
    p.catch(() => releases.delete(key));
    releases.set(key, p);
  }
  return p;
}
export const forgetRelease = (source: string, album: string) => void releases.delete(`${source}/${album}`);
export const searchTrack = (album: { source: string; id: string }, recording: string, replace?: { source: string; id: string }) =>
  call<{ id: string }>('/api/track-search', 'POST', { album, recording, ...(replace ? { replace } : {}) });
export const getSearch = (id: string) => call<Search>(`/api/track-search/${enc(id)}`);
/** An audio file in Cratarr's own download folder (0.10; administrators'): what its tags say, where they say it. */
export type TrackFile = { path: string; name: string; bytes: number; durationSecs?: number; artist?: string; album?: string; title?: string; trackNo?: number };
/** The files there, the best match for `q` first. */
export const getTrackFiles = (q: string) => call<TrackFile[]>(`/api/track-files?${new URLSearchParams({ q })}`);
/** A file taken for a missing track: copied (never moved) and checked as a Soulseek copy is, the same fetch from then on. */
export const fetchFile = (path: string, album: { source: string; id: string }, recording: string, replace?: { source: string; id: string }) =>
  call<{ id: string }>('/api/track-files/fetch', 'POST', { path, album, recording, ...(replace ? { replace } : {}) });
export const fetchCandidate = (search: string, candidate: string) => call<{ id: string }>(`/api/track-search/${enc(search)}/fetch`, 'POST', { candidate });
export const getFetch = (id: string) => call<Fetch>(`/api/track-fetch/${enc(id)}`);
export const dropFetch = (id: string) => call<void>(`/api/track-fetch/${enc(id)}`, 'DELETE');
/** A run on the one path (0.14): the copy chosen is the yes, its Undo the run's (lib/fileRuns whenWritten). */
export const addFetched = (id: string) => call<FileRun>(`/api/track-fetch/${enc(id)}/add`, 'POST', {});

// Many at once, you approve (0.13; the owner, 2026-10-31: "Bulk search, you approve", decision 0080 amended): the ticked
// albums' missing tracks looked for in turn (one Soulseek search at a time), the best copy of each chosen; nothing goes
// in until it is ticked and approved, each then fetched, checked and added as one is by hand (lib/trackIntake).
export type IntakeState = 'queued' | 'searching' | 'found' | 'none' | 'failed' | 'approved' | 'fetching' | 'checking' | 'added' | 'refused';
export type IntakeItem = {
  id: string;
  album: { source: string; id: string; title: string; artist: string | null };
  disc: number;
  position: number;
  title: string;
  recording: string;
  state: IntakeState;
  /** At most five, the best first. */
  candidates: Candidate[];
  /** The candidate chosen for it (the best), unless picked otherwise. */
  chosen: string | null;
  why?: string;
  /** Added: its run, which Undo takes out again (lib/fileRuns undoRun). */
  undo?: string;
  /** Not found (0.15, the retry ladder): searches in a row that found nothing, and when it is looked for again (epoch
   *  s; 0: due). Users who failed it, never asked again; its files set aside, each with why. */
  misses?: number;
  due?: number;
  failed?: string[];
  set_aside?: string[];
};
export type Intake = { running: boolean; items: IntakeItem[] };
export const startIntake = (albums: string[]) => call<Intake>('/api/track-intake', 'POST', { albums });
export const getIntake = () => call<Intake>('/api/track-intake');
export const approveIntake = (items: { id: string; candidate?: string }[]) => call<void>('/api/track-intake/approve', 'POST', { items });
export const dropIntake = (id: string) => call<void>(`/api/track-intake/${enc(id)}`, 'DELETE');
export const clearIntake = () => call<void>('/api/track-intake', 'DELETE');
/** Looked for now, whatever its wait (0.15: Search now). */
export const searchIntakeNow = (id: string) => call<Intake>(`/api/track-intake/${enc(id)}/now`, 'POST');
