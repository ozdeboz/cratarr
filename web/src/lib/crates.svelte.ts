// The permanent crates and their sizes (decision 0024), for the Crates page and the crate viewed in the panel. Asked for
// when looked at, and again when the playlists change (sorting a track into one shrinks Unlisted).
import { albumCoverUrl, crateImageUrl, crateRows, getCrates, removeCrateImage, setCrateFavouriteAlbums, setCrateFavourites, setCrateImage, type Condition, type Crate, type CrateFavourites, type PlaylistRow } from './api';
import type { LocalTrack } from './localPlayer.svelte';
import { playPlaylistFrom } from './output.svelte';
import { prefs } from './prefs.svelte';
import { viewPlaylist } from './panel.svelte';
import { toast } from './toast.svelte';

export const crates = $state<{ list: Crate[] | null; error: string | null }>({ list: null, error: null });

export async function loadCrates() {
  try {
    const list = await getCrates(prefs.source || undefined);
    // A tap still on its way to the server keeps showing.
    crates.list = list.map((c) => ({
      ...c,
      ...(c.id in pending ? { favourites: pending[c.id] } : {}),
      ...(c.id in pendingAlbums ? { favourite_albums: pendingAlbums[c.id] } : {}),
    }));
    crates.error = null;
    await moveBrowserSettings(list);
  } catch (e) {
    crates.error = e instanceof Error ? e.message : 'Unexpected error';
  }
}

// The favourites toggle, one setting per crate, kept on the server (decision 0024, amended): a crate not set yet
// shows no favourites. A tap shows at once; the setting follows to the server.
const pending: Record<string, CrateFavourites> = {};
let saving: Promise<unknown> = Promise.resolve();
export const favouritesOf = (id: string): CrateFavourites => crates.list?.find((c) => c.id === id)?.favourites ?? (genreOf(id) !== null ? 'all' : 'none');
/** Resolves when every tap so far (on either toggle) is on the server (Make playlist reads the settings there). */
export const favouritesSaved = () => saving;
/** How each setting shows, on the panel's toggle and the Crates page's rows: a variation on the heart. */
export const FAVOURITES = {
  all: { icon: 'heart', label: 'All tracks', filled: false },
  only: { icon: 'heart', label: 'Only favourites', filled: true },
  none: { icon: 'heartOff', label: 'No favourites', filled: false },
} as const;
const NEXT: Record<CrateFavourites, CrateFavourites> = { none: 'only', only: 'all', all: 'none' };
/** A tap on the toggle: No favourites, Only favourites, All tracks, and round again. */
export function cycleFavourites(id: string) {
  const c = crates.list?.find((x) => x.id === id);
  if (!c) return;
  const was = c.favourites;
  const next = NEXT[was];
  c.favourites = pending[id] = next;
  saving = saving.then(() =>
    setCrateFavourites(id, next).then(
      () => pending[id] === next && delete pending[id],
      (e) => {
        if (pending[id] !== next) return;
        delete pending[id];
        const shown = crates.list?.find((x) => x.id === id);
        if (shown) shown.favourites = was;
        toast.fail(e, "The crate's setting wasn't saved");
      },
    ),
  );
}

// The favourite albums toggle (decision 0024, amended 2026-10-19), beside the favourites one and as it is: one setting
// per crate on the server, a tap shown at once. A crate not set yet shows all albums. Its sizes are counted with it as
// set, so the crates are read again once a tap is saved.
const pendingAlbums: Record<string, CrateFavourites> = {};
export const favouriteAlbumsOf = (id: string): CrateFavourites => crates.list?.find((c) => c.id === id)?.favourite_albums ?? 'all';
/** How each setting shows: a variation on the album's disc. */
export const FAVOURITE_ALBUMS = {
  all: { icon: 'disc', label: 'All albums', filled: false },
  only: { icon: 'disc', label: 'Only favourite albums', filled: true },
  none: { icon: 'albumsOff', label: 'No favourite albums', filled: false },
} as const;
/** A tap on the albums toggle: No favourite albums, Only favourite albums, All albums, and round again. */
export function cycleFavouriteAlbums(id: string) {
  const c = crates.list?.find((x) => x.id === id);
  if (!c) return;
  const was = c.favourite_albums ?? 'all';
  const next = NEXT[was];
  c.favourite_albums = pendingAlbums[id] = next;
  saving = saving.then(() =>
    setCrateFavouriteAlbums(id, next).then(
      () => {
        if (pendingAlbums[id] !== next) return;
        delete pendingAlbums[id];
        loadCrates();
      },
      (e) => {
        if (pendingAlbums[id] !== next) return;
        delete pendingAlbums[id];
        const shown = crates.list?.find((x) => x.id === id);
        if (shown) shown.favourite_albums = was;
        toast.fail(e, "The crate's setting wasn't saved");
      },
    ),
  );
}

// Before the setting was the server's (2026-10-14), each browser kept its own: sent once, then forgotten here.
const OLD_KEY = 'cratarr-crate-favourites';
async function moveBrowserSettings(list: Crate[]) {
  let read: unknown = null;
  try {
    read = JSON.parse(localStorage.getItem(OLD_KEY) ?? 'null');
    localStorage.removeItem(OLD_KEY);
  } catch {
    return;
  }
  if (!read || typeof read !== 'object') return;
  const old = read as Record<string, CrateFavourites>;
  const moved = list.filter((c) => old[c.id] && old[c.id] !== c.favourites);
  for (const c of moved) {
    try {
      await setCrateFavourites(c.id, old[c.id]);
      const shown = crates.list?.find((x) => x.id === c.id);
      if (shown && !(c.id in pending)) shown.favourites = old[c.id];
    } catch {
      /* the server's setting stays */
    }
  }
}

/** A crate's id in the panel's address (`?view=crate:unlisted`), and back. */
export const CRATE = 'crate:';
export const crateOf = (viewed: string | null) => (viewed?.startsWith(CRATE) ? viewed.slice(CRATE.length) : null);

/** The builder (decision 0024, the panel's third mode): `?view=crate-new`, or `?view=crate-edit:<id>` for one's own crate.
 *  `{ id: null }` is a new crate. */
export const NEW_CRATE = 'crate-new';
export const EDIT_CRATE = 'crate-edit:';
export const builderOf = (viewed: string | null): { id: string | null } | null =>
  viewed === NEW_CRATE ? { id: null } : viewed?.startsWith(EDIT_CRATE) ? { id: viewed.slice(EDIT_CRATE.length) } : null;

/** A crate being made or changed, kept in this browser under its own address until Save or Cancel (it survives the
 *  player over it, a page change and a reload). */
export type CrateDraft = { name: string; conditions: Condition[] };
const draftKey = (id: string | null) => `cratarr-crate-draft:${id ?? 'new'}`;
export function keptDraft(id: string | null): CrateDraft | null {
  try {
    const kept = JSON.parse(localStorage.getItem(draftKey(id)) ?? 'null');
    return kept && Array.isArray(kept.conditions) ? kept : null;
  } catch {
    return null;
  }
}
export function keepDraft(id: string | null, draft: CrateDraft) {
  try {
    localStorage.setItem(draftKey(id), JSON.stringify(draft));
  } catch {
    /* not kept */
  }
}
export function forgetDraft(id: string | null) {
  try {
    localStorage.removeItem(draftKey(id));
  } catch {
    /* nothing kept */
  }
}
export const crateName = (id: string) => crates.list?.find((c) => c.id === id)?.name ?? genreOf(id) ?? id;

// A crate's picture (decided 2026-10-16): its own, else its albums' covers (four as a mosaic, else the first), on its
// card and over it in the panel. Its own is read again after a change (a new address).
const imageRounds = $state<Record<string, number>>({});
/** `own`: false leaves its own picture out (edit mode, about to remove it). */
export function crateArt(id: string, own = true): string[] {
  const c = crates.list?.find((x) => x.id === id);
  if (!c) return [];
  if (own && c.image) return [crateImageUrl(id, imageRounds[id] ?? 0)];
  const covers = (c.stats?.covers ?? []).map((x) => albumCoverUrl(x.source, x.album_id));
  return covers.length >= 4 ? covers.slice(0, 4) : covers.slice(0, 1);
}
/** Sets a crate's own picture (`file`), or removes it (`null`). */
export async function setCrateArt(id: string, file: Blob | null) {
  if (file) await setCrateImage(id, file);
  else await removeCrateImage(id);
  imageRounds[id] = Date.now();
  const c = crates.list?.find((x) => x.id === id);
  if (c) c.image = !!file;
}

/** A genre in the panel: a crate of that one genre, made by the server when asked for (`crate:genre:<name>`), never
 *  saved; reached from a track's menu and from the genres shown on top of a list. */
export const GENRE = 'genre:';
export const genreOf = (crate: string | null) => (crate?.startsWith(GENRE) ? crate.slice(GENRE.length) : null);
export const viewGenre = (name: string) => viewPlaylist(CRATE + GENRE + name);

/** A new crate, named in the name sheet (as a new playlist is): the builder opens on a fresh draft of that name, saved
 *  only with its Save (a crate without conditions would hold the whole library). `conditions`: what it starts from (a
 *  genre's own, from the genre's view). */
export function startCrate(name: string, conditions: Condition[] = []) {
  try {
    localStorage.setItem(`cratarr-crate-draft:new`, JSON.stringify({ name, conditions }));
  } catch {
    /* the builder starts empty */
  }
  viewPlaylist(NEW_CRATE);
}

// A crate shuffled from its card (the owner, 2026-10-19: "a play button on the crates, which automatically plays it with
// shuffle"): its tracks as both its toggles let them through, from anywhere in it, in a random order, as a new queue.
// A crate comes 500 tracks at a time, so a big one is sampled: up to three stretches of 500 from anywhere in it.
const STRETCH = 500;
const STRETCHES = 3;
const shuffled = <T>(list: T[]) => {
  const a = [...list];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
};
export async function shuffleCrate(id: string, name: string) {
  const source = prefs.source || undefined;
  const fav = favouritesOf(id);
  const alb = favouriteAlbumsOf(id);
  const first = await crateRows(id, 0, STRETCH, source, fav, alb);
  const all = Math.ceil(first.total / STRETCH);
  const picks = all <= STRETCHES ? [...Array(all).keys()] : shuffled([...Array(all).keys()]).slice(0, STRETCHES);
  const pages = await Promise.all(picks.map((p) => (p === 0 ? first : crateRows(id, p * STRETCH, STRETCH, source, fav, alb))));
  const tracks: LocalTrack[] = pages
    .flatMap((p) => p.rows)
    .filter((r: PlaylistRow) => r.id && !r.missing)
    .map((r: PlaylistRow) => ({ source: first.source, id: r.id!, name: r.name ?? r.file, artist: r.artist, album: r.album, album_id: r.album_id ?? null, duration_secs: r.duration_secs }));
  if (!tracks.length) throw new Error('Nothing in this crate to play');
  await playPlaylistFrom(name, first.source, shuffled(tracks), 0);
}
