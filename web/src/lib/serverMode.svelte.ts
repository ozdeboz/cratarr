// A server's mode (decision 0098): the page's views of a Jellyfin's or a Subsonic server's music, as local mode's are
// of the device's (lib/deviceMusic), its tracks' source `server:<id>`. The Records shelf, the panel's album, artist and
// playlist, Home's rows and search read it here; its covers come through the app (`/__server/cover/…`, with the
// sign-in it keeps), as the player's streams do, so no address with a sign-in in it is ever on the page.

import type { AlbumView, ArtistView, ListedAlbum, PlaylistRow } from './api';
import { appInfo, inApp } from './app.svelte';
import { localMode } from './deviceMusic.svelte';
import type { LocalTrack } from './localPlayer.svelte';
import { servers, serverOf } from './servers.svelte';
import type { AlbumOrder, HomeList, MusicServer, ServerAlbum, ServerArtist, ServerSignIn, ServerTrack } from './servers/server';

export const SERVER_PREFIX = 'server:';
/** A server's tracks' and albums' source, and its mode's name in the app. */
export const serverSource = (id: string) => `${SERVER_PREFIX}${id}`;
export const isServerSource = (source: string | null | undefined): source is string => !!source?.startsWith(SERVER_PREFIX);
export const serverIdOf = (source: string) => source.slice(SERVER_PREFIX.length);

/** The server whose mode is on, if one is. */
export function serverMode(): ServerSignIn | null {
  if (!inApp || !isServerSource(appInfo.mode)) return null;
  const id = serverIdOf(appInfo.mode);
  return servers.list?.find((s) => s.id === id) ?? null;
}
/** A mode of the device's own player: Local, or a server's; nothing of Cratarr's server (no visuals, no hearts of
 *  Cratarr's, no Cratarr radio). */
export const ownMode = () => localMode() || !!serverMode();

/** The reader of the server a source names; it is gone where that server was removed. */
export function readerOf(source: string): Promise<MusicServer> {
  const s = servers.list?.find((x) => x.id === serverIdOf(source));
  return s ? serverOf(s) : Promise.reject(new Error('That server is no longer signed in to here'));
}

// Covers by album id, as the albums' lists gave them (a Subsonic server names its covers apart from its albums).
const covers = new Map<string, string>();
/** An album's or a track's cover, through the app. */
export function serverCoverUrl(source: string, cover: string, size = 600) {
  return `/__server/cover/${encodeURIComponent(serverIdOf(source))}/${encodeURIComponent(cover)}?size=${size}`;
}
export const serverAlbumCoverUrl = (source: string, album: string) => serverCoverUrl(source, covers.get(`${source}/${album}`) ?? album);

// Artists by name, as their albums named them (the panel views an artist by name).
const artistIds = new Map<string, string>();

export function listed(source: string, a: ServerAlbum): ListedAlbum {
  if (a.cover) covers.set(`${source}/${a.id}`, a.cover);
  if (a.artist && a.artist_id) artistIds.set(`${source}/${a.artist}`, a.artist_id);
  return { id: a.id, title: a.title, artist: a.artist, year: a.year, added: a.added, path: null, tracks: a.tracks, format: null, bitrate_kbps: null, tags: [], duration_secs: a.duration_secs };
}

/** A server's track as the panel's rows show tracks. */
export const serverRow = (t: ServerTrack): PlaylistRow => ({
  entry: t.entry ?? t.id,
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

/** Its tracks as the player takes them. */
export const asPlayed = (source: string, t: ServerTrack): LocalTrack => ({ source, id: t.id, name: t.name, artist: t.artist, album: t.album, album_id: t.album_id, duration_secs: t.duration_secs });

/** The Records shelf: every album, read a page at a time. */
export async function serverAlbums(source: string, order: AlbumOrder = 'added'): Promise<ListedAlbum[]> {
  const r = await readerOf(source);
  const out: ListedAlbum[] = [];
  const PAGE = 500;
  for (let at = 0; ; at += PAGE) {
    const page = await r.albums(order, at, PAGE);
    out.push(...page.map((a) => listed(source, a)));
    if (page.length < PAGE || out.length >= 20_000) return out;
  }
}

/** One of Home's rows. */
export const serverList = async (source: string, which: HomeList, size = 20) => (await (await readerOf(source)).list(which, size)).map((a) => listed(source, a));

/** An album as the panel views one. */
export async function serverAlbumView(source: string, id: string): Promise<AlbumView & { total: number }> {
  const { album, tracks } = await (await readerOf(source)).album(id);
  listed(source, album);
  return {
    source,
    id,
    title: album.title,
    artist: album.artist,
    year: album.year,
    genres: [],
    format: null,
    bitrate_kbps: null,
    rows: tracks.map(serverRow),
    ...(album.added ? { added: album.added } : {}),
    total: tracks.length,
  };
}

/** An artist as the panel views one, by the name their albums gave (their id where it is known, else found by it). */
export async function serverArtistView(source: string, name: string): Promise<ArtistView> {
  const r = await readerOf(source);
  let id = artistIds.get(`${source}/${name}`);
  if (!id) {
    const found = (await r.search(name, 10)).artists.find((a: ServerArtist) => a.name.toLowerCase() === name.toLowerCase());
    if (!found) throw new Error(`${name} is not on this server`);
    id = found.id;
  }
  const { albums } = await r.artist(id);
  const listedAlbums = albums.map((a) => listed(source, a));
  const views = await Promise.all(albums.map((a) => r.album(a.id).catch(() => null)));
  const rows = views.flatMap((v) => v?.tracks ?? []).map(serverRow);
  return { source, name, albums: listedAlbums, appears_on: [], rows, favorite: false };
}

/** An album's tracks for the player, in its order. */
export async function serverAlbumTracks(source: string, ids: string[]): Promise<LocalTrack[]> {
  const r = await readerOf(source);
  const views = await Promise.all(ids.map((id) => r.album(id)));
  return views.flatMap((v) => v.tracks).map((t) => asPlayed(source, t));
}

/** Its playlists, as the Playlists page lists them. */
export const serverPlaylists = async (source: string) => (await readerOf(source)).playlists();

/** A playlist as the panel views one: its tracks, in its order, each with its place (`entry`, what an edit names). */
export async function serverPlaylistView(source: string, id: string): Promise<{ source: string; title: string; rows: PlaylistRow[]; total: number; cover: string | null }> {
  const { playlist, tracks } = await (await readerOf(source)).playlist(id);
  for (const t of tracks) if (t.album_id && t.cover) covers.set(`${source}/${t.album_id}`, t.cover);
  return { source, title: playlist.name, rows: tracks.map(serverRow), total: tracks.length, cover: playlist.cover ? serverCoverUrl(source, playlist.cover) : null };
}
