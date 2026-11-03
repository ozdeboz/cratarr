// A Subsonic server as a mode (decision 0098): Navidrome, Gonic, Airsonic and the rest, through subsonic-api (MIT; the
// owner, 2026-11-03: "add both"). Every answer is checked for Subsonic's own "failed" before it is read.

import { SubsonicAPI } from 'subsonic-api';
import type { AlbumID3, ArtistID3, Child, Playlist } from 'subsonic-api';
import { epoch, ServerError, type AlbumOrder, type HomeList, type MusicServer, type ServerAlbum, type ServerArtist, type ServerPlaylist, type ServerSignIn, type ServerTrack } from './server';

/** Subsonic's codes for a sign-in refused (wrong user or password, token or key). */
const SIGNED_OUT = new Set([40, 41, 42, 43, 44]);

type Answer = { status: 'ok' | 'failed'; error?: { code: number; message?: string } };

function ok<T extends Answer>(a: T | undefined): T {
  if (!a) throw new ServerError('The server gave no answer');
  if (a.status !== 'ok') throw new ServerError(a.error?.message || 'The server refused it', SIGNED_OUT.has(a.error?.code ?? 0));
  return a;
}

const ORDER: Record<AlbumOrder, 'alphabeticalByName' | 'alphabeticalByArtist' | 'newest' | 'byYear'> = {
  name: 'alphabeticalByName',
  artist: 'alphabeticalByArtist',
  added: 'newest',
  year: 'byYear',
};
const LIST: Record<HomeList, 'newest' | 'recent' | 'frequent' | 'starred' | 'random'> = {
  added: 'newest',
  played: 'recent',
  most: 'frequent',
  favourites: 'starred',
  random: 'random',
};

export const subsonicTrack = (c: Child): ServerTrack => ({
  id: c.id,
  name: c.title,
  artist: c.displayArtist ?? c.artist ?? null,
  artist_id: c.artistId ?? null,
  album: c.album ?? null,
  album_id: c.albumId ?? null,
  track: c.track ?? null,
  disc: c.discNumber ?? null,
  year: c.year ?? null,
  duration_secs: c.duration ?? null,
  added: epoch(c.created),
  starred: !!c.starred,
  cover: c.coverArt ?? null,
});

export const subsonicAlbum = (a: AlbumID3): ServerAlbum => ({
  id: a.id,
  title: a.name,
  artist: a.displayArtist ?? a.artist ?? null,
  artist_id: a.artistId ?? null,
  year: a.year ?? null,
  added: epoch(a.created),
  tracks: a.songCount,
  duration_secs: a.duration ?? null,
  starred: !!a.starred,
  cover: a.coverArt ?? null,
});

const subsonicArtist = (a: ArtistID3): ServerArtist => ({ id: a.id, name: a.name, albums: a.albumCount ?? null, starred: !!a.starred, cover: a.coverArt ?? null });

const subsonicPlaylist = (p: Playlist): ServerPlaylist => ({ id: p.id, name: p.name, tracks: p.songCount, duration_secs: p.duration ?? null, cover: p.coverArt ?? null });

function client(url: string, user: string, secret: string, apiKey: boolean, fetcher?: typeof fetch): SubsonicAPI {
  return new SubsonicAPI({ url, auth: apiKey ? { apiKey: secret } : { username: user, password: secret }, ...(fetcher ? { fetch: fetcher } : {}) });
}

/** Signs in: the server must answer a ping with this user. Its name is its own where it says one (OpenSubsonic's
 *  `type`, "navidrome"), else its address's host. */
export async function signInSubsonic(id: string, url: string, user: string, secret: string, apiKey = false, fetcher?: typeof fetch): Promise<ServerSignIn> {
  const ping = ok(await client(url, user, secret, apiKey, fetcher).ping());
  const kind = 'type' in ping && typeof ping.type === 'string' ? ping.type : null;
  const host = new URL(url.startsWith('http') ? url : `https://${url}`).hostname;
  return { id, kind: 'subsonic', name: kind ? `${kind[0].toUpperCase()}${kind.slice(1)} on ${host}` : host, url, user, secret, ...(apiKey ? { apiKey } : {}) };
}

export function subsonicServer(signIn: ServerSignIn, fetcher?: typeof fetch): MusicServer {
  const api = client(signIn.url, signIn.user, signIn.secret, !!signIn.apiKey, fetcher);
  /** A playlist's tracks with their places, as Subsonic names a place: its index. */
  const entries = (songs: Child[] | undefined) => (songs ?? []).map((c, i) => ({ ...subsonicTrack(c), entry: String(i) }));
  let mixes: boolean | null = null;

  return {
    kind: 'subsonic',
    signIn,

    async albums(order, offset, size) {
      const a = ok(await api.getAlbumList2({ type: ORDER[order], offset, size, ...(order === 'year' ? { fromYear: 9999, toYear: 0 } : {}) }));
      return (a.albumList2.album ?? []).map(subsonicAlbum);
    },
    async album(id) {
      const a = ok(await api.getAlbum({ id })).album;
      return { album: subsonicAlbum(a), tracks: (a.song ?? []).map(subsonicTrack) };
    },
    async artists() {
      const a = ok(await api.getArtists());
      return (a.artists.index ?? []).flatMap((i) => i.artist ?? []).map(subsonicArtist);
    },
    async artist(id) {
      const a = ok(await api.getArtist({ id })).artist;
      return { artist: subsonicArtist(a), albums: (a.album ?? []).map(subsonicAlbum) };
    },
    async search(words, size) {
      const r = ok(await api.search3({ query: words, artistCount: size, albumCount: size, songCount: size })).searchResult3;
      return { artists: (r.artist ?? []).map(subsonicArtist), albums: (r.album ?? []).map(subsonicAlbum), tracks: (r.song ?? []).map(subsonicTrack) };
    },
    async list(which, size) {
      return (ok(await api.getAlbumList2({ type: LIST[which], size })).albumList2.album ?? []).map(subsonicAlbum);
    },

    async playlists() {
      return (ok(await api.getPlaylists()).playlists.playlist ?? []).map(subsonicPlaylist);
    },
    async playlist(id) {
      const p = ok(await api.getPlaylist({ id })).playlist;
      return { playlist: subsonicPlaylist(p), tracks: entries(p.entry) };
    },
    async createPlaylist(name, tracks) {
      return ok(await api.createPlaylist({ name, songId: tracks })).playlist.id;
    },
    async renamePlaylist(id, name) {
      ok(await api.updatePlaylist({ playlistId: id, name }));
    },
    async addToPlaylist(id, tracks) {
      ok(await api.updatePlaylist({ playlistId: id, songIdToAdd: tracks }));
    },
    async removeFromPlaylist(id, gone) {
      ok(await api.updatePlaylist({ playlistId: id, songIndexToRemove: gone.map(Number) }));
    },
    // Subsonic has no move: the playlist is written again whole, its tracks in the new order, those left out gone
    // (createPlaylist with its id replaces its songs). Its entries are places, so it must be as it was read.
    async keepPlaylist(id, order) {
      const now = ok(await api.getPlaylist({ id })).playlist.entry ?? [];
      if (order.some((e) => !now[Number(e)])) throw new ServerError('The playlist changed on the server meanwhile');
      ok(await api.createPlaylist({ playlistId: id, songId: order.map((e) => now[Number(e)].id) }));
    },
    async deletePlaylist(id) {
      ok(await api.deletePlaylist({ id }));
    },

    // getSimilarSongs2 is optional: a server without it answers "failed" (Navidrome without an agent answers empty).
    async mix(track, size) {
      if (mixes === false) return null;
      const a = await api.getSimilarSongs2({ id: track, count: size }).catch(() => undefined);
      if (!a || a.status !== 'ok') {
        if (a?.error && SIGNED_OUT.has(a.error.code)) throw new ServerError(a.error.message || 'Signed out', true);
        mixes = false;
        return null;
      }
      mixes = true;
      return (a.similarSongs2.song ?? []).map(subsonicTrack);
    },

    async star(what, id, on) {
      const which = what === 'album' ? { albumId: id } : what === 'artist' ? { artistId: id } : { id };
      ok(await (on ? api.star(which) : api.unstar(which)));
    },
    async playing(track) {
      await api.scrobble({ id: track, submission: false }).catch(() => undefined);
    },
    async played(track, at) {
      ok(await api.scrobble({ id: track, submission: true, time: at * 1000 }));
    },

    streamUrl: (track) => api.streamURL({ id: track }),
    coverUrl: (cover, size) => api.getCoverArtURL({ id: cover, size }),
  };
}
