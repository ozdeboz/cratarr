// A Jellyfin as a mode (decision 0098), through Jellyfin's own SDK (@jellyfin/sdk, MPL-2.0; the owner, 2026-11-03:
// "add both"). Signed in by name and password, or by Quick Connect (a code approved in another of its apps), into an
// access token the app keeps; the password itself is never kept.

import { Jellyfin, type Api } from '@jellyfin/sdk';
import type { BaseItemDto } from '@jellyfin/sdk/lib/generated-client/models/index.js';
import { getAuthenticationApi } from '@jellyfin/sdk/lib/utils/api/authentication-api.js';
import { getImageApi } from '@jellyfin/sdk/lib/utils/api/image-api.js';
import { getInstantMixApi } from '@jellyfin/sdk/lib/utils/api/instant-mix-api.js';
import { getLibraryApi } from '@jellyfin/sdk/lib/utils/api/library-api.js';
import { getArtistApi } from '@jellyfin/sdk/lib/utils/api/artist-api.js';
import { getPlaylistApi } from '@jellyfin/sdk/lib/utils/api/playlist-api.js';
import { getSessionApi } from '@jellyfin/sdk/lib/utils/api/session-api.js';
import { getSystemApi } from '@jellyfin/sdk/lib/utils/api/system-api.js';
import { getUserDataApi } from '@jellyfin/sdk/lib/utils/api/user-data-api.js';
import type { AxiosInstance } from 'axios';
import { epoch, ServerError, type AlbumOrder, type HomeList, type MusicServer, type ServerAlbum, type ServerArtist, type ServerPlaylist, type ServerSignIn, type ServerTrack } from './server';

/** How Cratarr names itself to a Jellyfin (its Dashboard's devices list). */
export type JellyfinClient = { version: string; device: string; deviceId: string };

function sdk(c: JellyfinClient): Jellyfin {
  return new Jellyfin({ clientInfo: { name: 'Cratarr', version: c.version }, deviceInfo: { name: c.device, id: c.deviceId } });
}

/** A Jellyfin's refusal as the page says it: 401 and 403 are a sign-in no longer taken. */
async function j<T>(call: Promise<{ data: T }>): Promise<T> {
  try {
    return (await call).data;
  } catch (e) {
    const status = (e as { response?: { status?: number } }).response?.status;
    if (status === 401 || status === 403) throw new ServerError('The server no longer takes this sign-in', true);
    throw new ServerError(status ? `The server refused it (${status})` : "Can't reach the server");
  }
}

/** Jellyfin's times are ticks of 100 ns. */
const secs = (ticks: number | null | undefined) => (ticks ? Math.round(ticks / 10_000_000) : null);

export const jellyfinTrack = (i: BaseItemDto): ServerTrack => ({
  id: i.Id ?? '',
  name: i.Name ?? '',
  artist: i.Artists?.join(', ') || i.AlbumArtist || null,
  artist_id: i.ArtistItems?.[0]?.Id ?? null,
  album: i.Album ?? null,
  album_id: i.AlbumId ?? null,
  track: i.IndexNumber ?? null,
  disc: i.ParentIndexNumber ?? null,
  year: i.ProductionYear ?? null,
  duration_secs: secs(i.RunTimeTicks),
  added: epoch(i.DateCreated),
  starred: !!i.UserData?.IsFavorite,
  cover: i.ImageTags?.Primary ? (i.Id ?? null) : (i.AlbumId ?? null),
  ...(i.PlaylistItemId ? { entry: i.PlaylistItemId } : {}),
});

export const jellyfinAlbum = (i: BaseItemDto): ServerAlbum => ({
  id: i.Id ?? '',
  title: i.Name ?? '',
  artist: i.AlbumArtist ?? i.AlbumArtists?.map((a) => a.Name).join(', ') ?? null,
  artist_id: i.AlbumArtists?.[0]?.Id ?? null,
  year: i.ProductionYear ?? null,
  added: epoch(i.DateCreated),
  tracks: i.ChildCount ?? i.SongCount ?? 0,
  duration_secs: secs(i.RunTimeTicks),
  starred: !!i.UserData?.IsFavorite,
  cover: i.ImageTags?.Primary ? (i.Id ?? null) : null,
});

const jellyfinArtist = (i: BaseItemDto): ServerArtist => ({
  id: i.Id ?? '',
  name: i.Name ?? '',
  albums: i.AlbumCount ?? null,
  starred: !!i.UserData?.IsFavorite,
  cover: i.ImageTags?.Primary ? (i.Id ?? null) : null,
});

const jellyfinPlaylist = (i: BaseItemDto): ServerPlaylist => ({
  id: i.Id ?? '',
  name: i.Name ?? '',
  tracks: i.ChildCount ?? 0,
  duration_secs: secs(i.RunTimeTicks),
  cover: i.ImageTags?.Primary ? (i.Id ?? null) : null,
});

/** Its name, as its Dashboard says it, else its address's host. */
async function serverName(api: Api, url: string): Promise<string> {
  const info = await j(getSystemApi(api).getPublicSystemInfo()).catch(() => null);
  return info?.ServerName || new URL(url).hostname;
}

/** Signs in by name and password, into an access token. */
export async function signInJellyfin(id: string, url: string, user: string, password: string, c: JellyfinClient, axios?: AxiosInstance): Promise<ServerSignIn> {
  const api = sdk(c).createApi(url, undefined, axios);
  const r = await j(getAuthenticationApi(api).authenticateUserByName({ authenticateUserByName: { Username: user, Pw: password } }));
  if (!r.AccessToken || !r.User?.Id) throw new ServerError('The server refused the sign-in', true);
  api.update({ accessToken: r.AccessToken });
  return { id, kind: 'jellyfin', name: await serverName(api, url), url, user: r.User.Name ?? user, userId: r.User.Id, secret: r.AccessToken };
}

/** Quick Connect: a code to approve in another of the Jellyfin's apps, then `wait` until it is (or `stop`). Null where
 *  the server has Quick Connect off. */
export async function quickConnect(id: string, url: string, c: JellyfinClient, axios?: AxiosInstance): Promise<{ code: string; wait: () => Promise<ServerSignIn>; stop: () => void } | null> {
  const api = sdk(c).createApi(url, undefined, axios);
  const auth = getAuthenticationApi(api);
  const on = await j(auth.getQuickConnectEnabled()).catch(() => false);
  if (!on) return null;
  const start = await j(auth.initiateQuickConnect());
  if (!start.Secret || !start.Code) return null;
  const secret = start.Secret;
  let stopped = false;
  return {
    code: start.Code,
    stop: () => void (stopped = true),
    async wait() {
      for (;;) {
        await new Promise((r) => setTimeout(r, 3000));
        if (stopped) throw new ServerError('Stopped');
        const s = await j(auth.getQuickConnectState({ secret }));
        if (!s.Authenticated) continue;
        const r = await j(auth.authenticateWithQuickConnect({ quickConnectDto: { Secret: secret } }));
        if (!r.AccessToken || !r.User?.Id) throw new ServerError('The server refused the sign-in', true);
        api.update({ accessToken: r.AccessToken });
        return { id, kind: 'jellyfin', name: await serverName(api, url), url, user: r.User.Name ?? '', userId: r.User.Id, secret: r.AccessToken };
      }
    },
  };
}

const FIELDS = ['ChildCount', 'DateCreated', 'PrimaryImageAspectRatio'] as const;

export function jellyfinServer(signIn: ServerSignIn, c: JellyfinClient, axios?: AxiosInstance): MusicServer {
  const api = sdk(c).createApi(signIn.url, signIn.secret, axios);
  const userId = signIn.userId;
  const library = getLibraryApi(api);
  const playlists = getPlaylistApi(api);
  const items = (q: Parameters<typeof library.getItems>[0]) => j(library.getItems({ userId, recursive: true, fields: [...FIELDS], ...q })).then((r) => r.Items ?? []);
  const albumsBy = (q: Parameters<typeof library.getItems>[0]) => items({ includeItemTypes: ['MusicAlbum'], ...q }).then((r) => r.map(jellyfinAlbum));
  /** The albums of the tracks a list finds, in its order, each once (a Jellyfin counts plays on tracks, not albums). */
  async function albumsOfTracks(q: Parameters<typeof library.getItems>[0], size: number): Promise<ServerAlbum[]> {
    const tracks = await items({ includeItemTypes: ['Audio'], limit: size * 4, ...q });
    const ids = [...new Set(tracks.map((t) => t.AlbumId).filter((x): x is string => !!x))].slice(0, size);
    if (!ids.length) return [];
    const by = new Map((await albumsBy({ ids })).map((a) => [a.id, a]));
    return ids.map((id) => by.get(id)).filter((a): a is ServerAlbum => !!a);
  }
  const ORDER: Record<AlbumOrder, { sortBy: ('SortName' | 'AlbumArtist' | 'DateCreated' | 'ProductionYear')[]; sortOrder: ('Ascending' | 'Descending')[] }> = {
    name: { sortBy: ['SortName'], sortOrder: ['Ascending'] },
    artist: { sortBy: ['AlbumArtist', 'SortName'], sortOrder: ['Ascending'] },
    added: { sortBy: ['DateCreated'], sortOrder: ['Descending'] },
    year: { sortBy: ['ProductionYear', 'SortName'], sortOrder: ['Descending'] },
  };
  async function playlistTracks(id: string) {
    return j(playlists.getPlaylistItems({ playlistId: id, userId })).then((r) => (r.Items ?? []).map(jellyfinTrack));
  }

  return {
    kind: 'jellyfin',
    signIn,

    albums: (order, offset, size) => albumsBy({ ...ORDER[order], startIndex: offset, limit: size }),
    async album(id) {
      const [a] = await albumsBy({ ids: [id] });
      if (!a) throw new ServerError('Not on the server any more');
      const tracks = await items({ parentId: id, includeItemTypes: ['Audio'], sortBy: ['ParentIndexNumber', 'IndexNumber', 'SortName'] });
      return { album: a, tracks: tracks.map(jellyfinTrack) };
    },
    async artists() {
      return j(getArtistApi(api).getAlbumArtists({ userId, fields: [...FIELDS] })).then((r) => (r.Items ?? []).map(jellyfinArtist));
    },
    async artist(id) {
      const a = await j(library.getItem({ itemId: id, userId }));
      return { artist: jellyfinArtist(a), albums: await albumsBy({ albumArtistIds: [id], sortBy: ['ProductionYear', 'SortName'], sortOrder: ['Descending'] }) };
    },
    async search(words, size) {
      const [artists, albums, tracks] = await Promise.all([
        j(getArtistApi(api).getAlbumArtists({ userId, searchTerm: words, limit: size })).then((r) => (r.Items ?? []).map(jellyfinArtist)),
        albumsBy({ searchTerm: words, limit: size }),
        items({ searchTerm: words, includeItemTypes: ['Audio'], limit: size }).then((r) => r.map(jellyfinTrack)),
      ]);
      return { artists, albums, tracks };
    },
    list(which, size) {
      switch (which) {
        case 'added':
          return albumsBy({ sortBy: ['DateCreated'], sortOrder: ['Descending'], limit: size });
        case 'played':
          return albumsOfTracks({ sortBy: ['DatePlayed'], sortOrder: ['Descending'], filters: ['IsPlayed'] }, size);
        case 'most':
          return albumsOfTracks({ sortBy: ['PlayCount'], sortOrder: ['Descending'], filters: ['IsPlayed'] }, size);
        case 'favourites':
          return albumsBy({ isFavorite: true, sortBy: ['SortName'], limit: size });
        case 'random':
          return albumsBy({ sortBy: ['Random'], limit: size });
      }
    },

    async playlists() {
      return items({ includeItemTypes: ['Playlist'], mediaTypes: ['Audio'], sortBy: ['SortName'] }).then((r) => r.map(jellyfinPlaylist));
    },
    async playlist(id) {
      const p = await j(library.getItem({ itemId: id, userId }));
      return { playlist: jellyfinPlaylist(p), tracks: await playlistTracks(id) };
    },
    async createPlaylist(name, ids) {
      const r = await j(playlists.createPlaylist({ createPlaylistDto: { Name: name, Ids: ids, UserId: userId, MediaType: 'Audio' } }));
      if (!r.Id) throw new ServerError('The server made no playlist');
      return r.Id;
    },
    async renamePlaylist(id, name) {
      await j(playlists.updatePlaylist({ playlistId: id, updatePlaylistDto: { Name: name } }));
    },
    async addToPlaylist(id, ids) {
      await j(playlists.addItemToPlaylist({ playlistId: id, ids, userId }));
    },
    async removeFromPlaylist(id, entries) {
      await j(playlists.removeItemFromPlaylist({ playlistId: id, entryIds: entries }));
    },
    // Those left out taken out, then the rest moved one entry at a time, each to its new place, front to back.
    async keepPlaylist(id, order) {
      let now = (await playlistTracks(id)).map((t) => t.entry ?? '');
      if (order.some((e) => !now.includes(e))) throw new ServerError('The playlist changed on the server meanwhile');
      const gone = now.filter((e) => !order.includes(e));
      if (gone.length) await j(playlists.removeItemFromPlaylist({ playlistId: id, entryIds: gone }));
      now = now.filter((e) => order.includes(e));
      for (let i = 0; i < order.length; i++) {
        if (now[i] === order[i]) continue;
        await j(playlists.moveItem({ playlistId: id, itemId: order[i], newIndex: i }));
        now.splice(now.indexOf(order[i]), 1);
        now.splice(i, 0, order[i]);
      }
    },
    async deletePlaylist(id) {
      await j(library.deleteItem({ itemId: id }));
    },

    async mix(track, size) {
      return j(getInstantMixApi(api).getInstantMixFromItem({ itemId: track, userId, limit: size })).then((r) => (r.Items ?? []).map(jellyfinTrack));
    },

    async star(_what, id, on) {
      const data = getUserDataApi(api);
      await j(on ? data.markFavoriteItem({ itemId: id, userId }) : data.unmarkFavoriteItem({ itemId: id, userId }));
    },
    async playing(track) {
      await j(getSessionApi(api).reportPlaybackStart({ playbackStartInfo: { ItemId: track, CanSeek: true } })).catch(() => undefined);
    },
    async played(track, at) {
      await j(getUserDataApi(api).markPlayedItem({ itemId: track, userId, datePlayed: new Date(at * 1000).toISOString() }));
    },

    // The file as it is (`static`): the app's player decodes what Jellyfin keeps.
    streamUrl: async (track) => api.getUri(`/Audio/${encodeURIComponent(track)}/stream`, { static: true, api_key: signIn.secret, DeviceId: c.deviceId }),
    coverUrl: async (cover, size) => getImageApi(api).getItemImageUrlById(cover, 'Primary', { fillWidth: size, fillHeight: size, quality: 90 }),
  };
}
