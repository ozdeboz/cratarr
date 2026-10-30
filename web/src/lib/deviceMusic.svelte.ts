// Local mode (decision 0052): in the app, the device's own music, in a mode of its own beside Cratarr's, never mixed
// (the owner: "mixing it without having all the features is weird"), each mode with its own queue. The app finds it
// (Android's media index, only in the folders turned on in Settings → This device) and plays it; the page shows its
// albums, artists and tracks, built here from the tracks the app lists, and switches the mode (the sidebar's switcher,
// lib/ModeSwitcher). Nothing of it reaches Cratarr's server: no hearts, playlists, crates, radio or Insights.

import type { AlbumView, ArtistView, ListedAlbum, PlaylistRow } from './api';
import { appInfo, inApp, onApp, toApp } from './app.svelte';
import type { LocalTrack } from './localPlayer.svelte';

/** A track of the device's, as the app lists it: `track` its number on its album. */
export type DeviceTrack = {
  source: 'device';
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  track: number | null;
  disc: number | null;
  year: number | null;
  duration_secs: number | null;
  /** When it came onto the device, epoch seconds. */
  added: number | null;
};
/** A folder Android found music in (its top folder: "Music/"), and whether local mode reads it. */
export type DeviceFolder = { path: string; name: string; tracks: number; on: boolean };

export const DEVICE = 'device';

/** The library (newest first) and the folders, as the app last told them; `denied`: Android's permission refused. */
export const device = $state({
  tracks: null as DeviceTrack[] | null,
  folders: null as DeviceFolder[] | null,
  denied: false,
});

/** Local mode is on (the app plays the device's own music). */
export const localMode = () => inApp && appInfo.mode === 'device';
/** The switcher shows: a folder is on and the permission given (the owner: only "if the user has enabled any local
 *  directories"); never in a browser. */
export const switchable = () => inApp && appInfo.local;

export function setMode(mode: 'cratarr' | 'device') {
  if (mode === appInfo.mode) return;
  navigator.vibrate?.(8);
  toApp({ type: 'device-music', op: 'mode', mode });
}
export const askLibrary = () => toApp({ type: 'device-music', op: 'library' });
export const askFolders = () => toApp({ type: 'device-music', op: 'folders' });
/** Android's permission dialog. */
export const allowDeviceMusic = () => toApp({ type: 'device-music', op: 'allow' });
/** Android's page of the app, where its permission to read the device's music is taken back. */
export const deviceMusicSettings = () => toApp({ type: 'device-music', op: 'settings' });
export function setFolder(path: string, on: boolean) {
  const f = device.folders?.find((x) => x.path === path);
  if (f) f.on = on;
  toApp({ type: 'device-music', op: 'folder', path, on });
}

onApp('device-library', (m) => {
  device.denied = m.denied === true;
  device.tracks = Array.isArray(m.tracks) ? (m.tracks as DeviceTrack[]) : [];
});
onApp('device-folders', (m) => {
  device.denied = m.denied === true;
  device.folders = Array.isArray(m.folders) ? (m.folders as DeviceFolder[]) : [];
});
// Android's index changed: read again while local mode shows it.
onApp('device-library-changed', () => {
  if (localMode()) askLibrary();
});

/** Its tracks as the player takes them. */
export const asLocal = (t: DeviceTrack): LocalTrack => ({ source: DEVICE, id: t.id, name: t.name, artist: t.artist, album: t.album, album_id: t.album_id, duration_secs: t.duration_secs });

/** The device's albums, as the Library's shelf lists albums: the most common artist, the newest track's date. */
export const deviceAlbums = (): ListedAlbum[] => {
  const by = new Map<string, DeviceTrack[]>();
  for (const t of device.tracks ?? []) if (t.album_id) by.set(t.album_id, [...(by.get(t.album_id) ?? []), t]);
  return [...by].map(([id, ts]) => ({
    id,
    title: ts[0].album ?? 'Unknown album',
    artist: mostOf(ts.map((t) => t.artist)),
    year: ts.find((t) => t.year)?.year ?? null,
    added: Math.max(...ts.map((t) => t.added ?? 0)) || null,
    path: null,
    tracks: ts.length,
    format: null,
    bitrate_kbps: null,
    tags: [],
  }));
};
function mostOf(names: (string | null)[]): string | null {
  const n = new Map<string, number>();
  for (const x of names) if (x) n.set(x, (n.get(x) ?? 0) + 1);
  return [...n].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
}

/** The device's artists by their tracks' artist: how many albums and tracks each has. */
export const deviceArtists = (): { name: string; albums: number; tracks: number }[] => {
  const by = new Map<string, DeviceTrack[]>();
  for (const t of device.tracks ?? []) if (t.artist) by.set(t.artist, [...(by.get(t.artist) ?? []), t]);
  return [...by]
    .map(([name, ts]) => ({ name, albums: new Set(ts.map((t) => t.album_id)).size, tracks: ts.length }))
    .sort((a, b) => a.name.localeCompare(b.name));
};

/** A device track as the panel's rows show tracks. */
export const deviceRow = (t: DeviceTrack): PlaylistRow => ({
  entry: t.id,
  file: t.name,
  id: t.id,
  name: t.name,
  artist: t.artist,
  album: t.album,
  duration_secs: t.duration_secs,
  missing: false,
  ...(t.track ? { track: t.track } : {}),
  ...(t.disc ? { disc: t.disc } : {}),
  ...(t.year ? { year: t.year } : {}),
  ...(t.album_id ? { album_id: t.album_id } : {}),
  ...(t.added ? { added: t.added } : {}),
});
const byPlace = (a: DeviceTrack, b: DeviceTrack) => (a.disc ?? 1) - (b.disc ?? 1) || (a.track ?? 0) - (b.track ?? 0) || a.name.localeCompare(b.name);

/** A device album as the panel views an album: its tracks in disc and track order. */
export function deviceAlbumView(id: string): AlbumView & { total: number } {
  const ts = (device.tracks ?? []).filter((t) => t.album_id === id).sort(byPlace);
  const a = deviceAlbums().find((x) => x.id === id);
  return {
    source: DEVICE,
    id,
    title: a?.title ?? 'Unknown album',
    artist: a?.artist ?? null,
    year: a?.year ?? null,
    format: null,
    bitrate_kbps: null,
    added: a?.added ?? null,
    played: null,
    plays: null,
    rows: ts.map(deviceRow),
    genres: null,
    total: ts.length,
  } as unknown as AlbumView & { total: number };
}

/** A device artist as the panel views an artist: their albums (newest first) and those albums' tracks of theirs, in
 *  each album's order; no heart (`favorite` absent). */
export function deviceArtistView(name: string): ArtistView {
  const ts = (device.tracks ?? []).filter((t) => t.artist === name);
  const ids = new Set(ts.map((t) => t.album_id));
  const albums = deviceAlbums()
    .filter((a) => ids.has(a.id))
    .sort((a, b) => (b.year ?? 0) - (a.year ?? 0));
  const rows = [...albums.map((a) => a.id), null].flatMap((id) => ts.filter((t) => t.album_id === id).sort(byPlace)).map(deviceRow);
  return { source: DEVICE, name, albums, appears_on: [], rows } as unknown as ArtistView;
}
