// What the album lists (Incoming, Collection) share: playing an album, and how its facts read.
import { trackSteps } from './progress.svelte';
import { favouriteOf } from './changeLog';
import { on } from './events.svelte';
import { leavePlayer } from './navStep.svelte';
import {
  albumView,
  getAlbumRating,
  getAlbums,
  getAlbumUses,
  setAlbumRating,
  type AlbumsView,
  type AlbumUse,
  type ListedAlbum,
  albumsTracks,
  ApiError,
} from './api';
import type { DownloadedTrack } from './app.svelte';
import { player } from './live.svelte';
import type { LocalTrack } from './localPlayer.svelte';
import { playAlbumHere, playPlaylistFrom } from './output.svelte';
import { choosePlayer, port } from './player/port.svelte';
import { artistSubject, viewOver } from './panel.svelte';
import { BANDCAMP } from './bandcampPreview';
import { cameFrom, DISCOVER } from './discoverArtist';
import { releaseGroupOfBandcamp } from './bandcampFound.svelte';
import { prefs } from './prefs.svelte';
import { libraryShown } from './sources.svelte';
import { toast } from './toast.svelte';
import { can } from './capabilities.svelte';

/** The Library's list (GET /api/albums) as last read: the page shows it, and the album panel's menu finds the album's
 *  library there (what it may do with it). */
export const library = $state({ view: null as AlbumsView | null });
let reading: Promise<void> | null = null;
/** Reads it (`fresh`: past the service worker's copy); one read at a time. */
export function readLibrary(fresh: boolean): Promise<void> {
  reading ??= getAlbums(prefs.source || undefined, fresh)
    .then((v) => {
      library.view = v;
    })
    .finally(() => (reading = null));
  return reading;
}

// The Library's albums by server: their ids, and the albums of each title. Made once per list read (the downloads
// look theirs up on every report of the app's).
type Index = Map<string, { ids: Set<string>; titled: Map<string, ListedAlbum[]>; library: Map<string, string> }>;
const indexes = new WeakMap<AlbumsView, Index>();
function indexOf(view: AlbumsView): Index {
  let index = indexes.get(view);
  if (!index) {
    index = new Map();
    for (const s of view.sources) {
      const titled = new Map<string, ListedAlbum[]>();
      for (const a of s.albums) titled.set(a.title, [...(titled.get(a.title) ?? []), a]);
      const library = new Map<string, string>();
      for (const a of s.albums) if (a.library) library.set(a.id, a.library);
      index.set(s.id, { ids: new Set(s.albums.map((a) => a.id)), titled, library });
    }
    indexes.set(view, index);
  }
  return index;
}

/** An album's library as a dot in its colour, wherever an album shows (decision 0034; 0.9.3, the owner: album cards,
 *  the album panel's header, search results): none for the primary (Music), none while only one of the server's
 *  libraries shows. Its library its own where it says, else the Library's list's (none until that is read). */
export function libraryDot(source: string, album: { id: string; library?: string }): { colour: string; name: string } | null {
  const s = library.view?.sources.find((x) => x.id === source);
  const info = s?.library_info;
  if (!info || info.filter((l) => libraryShown(source, l.id)).length < 2) return null;
  const lib = album.library ?? indexOf(library.view!).get(source)?.library.get(album.id);
  const l = info.find((x) => x.id === lib);
  return l?.colour ? { colour: l.colour, name: l.name } : null;
}

/** A track downloaded in the app: its album in the Library. A download's own album id where the Library lists it;
 *  else (a download from before a playlist's rows said their albums, or an album since moved) the Library's album of
 *  that name on its server, the track's artist's when several have the name. Else its own id, or null. */
export function albumOfDownload(t: DownloadedTrack): string | null {
  const listed = library.view && indexOf(library.view).get(t.source);
  if (!listed || (t.album_id && listed.ids.has(t.album_id)) || !t.album) return t.album_id;
  const named = listed.titled.get(t.album) ?? [];
  const found = named.length > 1 ? named.filter((a) => a.artist === t.artist || (!!t.artist && !!a.artists?.includes(t.artist))) : named;
  return found.length === 1 ? found[0].id : t.album_id;
}

/** The album being started (its Play buttons wait meanwhile). */
export const albumPlay = $state({ starting: null as string | null });

/** Plays an album where the page plays (decision 0022); with no player to play on, it says so (0.12). */
export async function playListedAlbum(source: string, a: Pick<ListedAlbum, 'id' | 'title'>) {
  if (albumPlay.starting) return;
  albumPlay.starting = a.id;
  try {
    if (!(await playAlbumHere(source, a.id, a.title))) choosePlayer();
  } catch (e) {
    toast.fail(e, 'Could not start playing');
  } finally {
    albumPlay.starting = null;
  }
}

/** Several albums' tracks, one album after another, in each album's order (an album's select bar). */
export async function tracksOfAlbums(list: { source: string; a: ListedAlbum }[]): Promise<LocalTrack[]> {
  // One call for them all (0.9.4: thousands of album views at once timed out, "cannot reach", the owner 2026-10-25),
  // not asked where the server's list (0.11) says it has no such call.
  if (can('batches') !== false)
    try {
      const { tracks } = await albumsTracks(list.map(({ source, a }) => ({ source, id: a.id })));
      return tracks.map((t) => ({ source: t.source, id: t.id, name: t.name, artist: t.artist ?? null, album: t.album ?? null, album_id: t.album_id, duration_secs: t.duration_secs ?? null }));
    } catch (e) {
      // A server before it (no such call): album by album as below; anything else is the error it is.
      if (!(e instanceof ApiError && (e.status === 404 || e.status === 405))) throw e;
    }
  // A few albums at a time: thousands at once waited in the browser's line until they timed out.
  const views: { r: Awaited<ReturnType<typeof albumView>>; a: ListedAlbum }[] = new Array(list.length);
  let next = 0;
  // The bar at the top says how far it is (lib/progress).
  await trackSteps(list.length, (step) =>
    Promise.all(
      Array.from({ length: Math.min(6, list.length) }, async () => {
        while (next < list.length) {
          const i = next++;
          const { source, a } = list[i];
          views[i] = { r: await albumView(source, a.id), a };
          step();
        }
      }),
    ),
  );
  return views.flatMap(({ r, a }) =>
    r.rows
      .filter((x) => x.id && !x.missing)
      .map((x) => ({ source: r.source, id: x.id!, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: a.id, duration_secs: x.duration_secs })),
  );
}

/** Plays, puts next or queues several albums, one after another: here when the output is this browser (decision
 *  0022), else on the server's player (as the panel's ticked tracks). */
export async function playAlbums(list: { source: string; a: ListedAlbum }[], mode: 'play' | 'next' | 'last') {
  const tracks = await tracksOfAlbums(list);
  if (!tracks.length) throw new Error('Those albums have no tracks.');
  const source = tracks[0].source;
  if (mode === 'play') await playPlaylistFrom(list.length === 1 ? list[0].a.title : `${list.length} albums`, source, tracks, 0);
  else await port().enqueue(tracks, mode, source);
}

/** An album artist's credits, lower case: "A & B, C feat. D" is a, b, c and d (and the credit whole). */
export const artistCredits = (artist: string | null) =>
  (artist ?? '')
    .toLowerCase()
    .split(/\s*(?:,|;|\/|&|\band\b|\bx\b|\bfeat\.?|\bft\.|\bfeaturing\b|\bvs\.?|\bwith\b)\s*/)
    .map((c) => c.trim())
    .filter(Boolean)
    .concat((artist ?? '').toLowerCase().trim());

/** Whom a credit is under: "A feat. B" is A's (the albums are under the main artist). */
export const mainArtist = (artist: string | null | undefined) => artist?.split(/\s+(?:feat\.?|ft\.|featuring)\s+/i)[0].trim() || null;

/** Go to artist: the artist's panel (decided 2026-10-16, as Symfonium's), over what the panel views (back returns to
 *  it). From the full-screen player, which closes, or it would cover the panel. */
export function goToArtist(source: string, name: string, from: { id: string; title: string; year?: number | null } | null = null) {
  // The player over the page closes once the artist shows (lib/navStep), or it would cover them.
  if (player.full) leavePlayer();
  // From a Bandcamp album (`from`): kept for their page from Discover (lib/discoverArtist cameFrom).
  if (from) Object.assign(cameFrom, { name, album: { id: from.id, title: from.title, year: from.year ?? null, release_group: releaseGroupOfBandcamp(from.id) } });
  // From something on Bandcamp (an album, a preview's track): no server of that name (the owner, 2026-10-25: it said
  // "server is not connected"), so the library's page where it has the artist, else theirs from Discover.
  if (source === BANDCAMP) source = hasArtist(name) ? 'cratarr' : DISCOVER;
  viewOver(artistSubject(source, name));
}

/** Whether Cratarr's own library has albums by the artist (as far as the library is read). */
function hasArtist(name: string): boolean {
  const n = name.toLowerCase();
  return (library.view?.sources ?? []).some((s) => s.id === 'cratarr' && s.albums.some((a) => a.artist?.toLowerCase() === n));
}

// How the owner used the albums (decided 2026-10-16): plays, last played and rating, per server, for the Library's
// orders (Plays, Last played, Rating); the filter's switches are the server's (lib/listFilter). Read when one is chosen,
// and again each time (plays change).
export const albumUses = $state<Record<string, Record<string, AlbumUse>>>({});
const NOT_USED: AlbumUse = { plays: 0, rating: 0 };
export const useOf = (source: string, id: string): AlbumUse => albumUses[source]?.[id] ?? NOT_USED;
export async function loadAlbumUses(source: string) {
  // The device's own music (local mode, decision 0052) is none of Cratarr's: no plays to ask about.
  if (source === 'device') return;
  try {
    albumUses[source] = await getAlbumUses(source);
  } catch {
    /* the orders fall back to the albums' own order */
  }
}

// An album's rating, the one its menu and its About show (lib/RatingStars): `source/id` → 0–5, null where the server
// keeps none (no stars then).
export const albumRatings = $state<Record<string, number | null>>({});
export function loadAlbumRating(source: string, id: string) {
  getAlbumRating(source, id).then(
    (r) => (albumRatings[`${source}/${id}`] = r.rating),
    () => {},
  );
}
// Ratings changed elsewhere while this page was away (decision 0087, lib/changes): those shown here asked for again.
on<{ area: string; ids?: string[] | null }>('changed', ({ area, ids }) => {
  if (area !== 'ratings') return;
  const named = ids?.flatMap((i) => {
    const x = favouriteOf(i);
    return x && x.what === 'album' ? [`${x.source}/${x.id}`] : [];
  });
  for (const key of named ?? Object.keys(albumRatings)) {
    if (!(key in albumRatings)) continue;
    const slash = key.indexOf('/');
    loadAlbumRating(key.slice(0, slash), key.slice(slash + 1));
  }
});
/** A tap on a star: that rating, or none when it is the one it has; shown at once, back as it was when refused. */
export async function rateAlbum(source: string, id: string, stars: number) {
  const key = `${source}/${id}`;
  const was = albumRatings[key] ?? 0;
  const now = was === stars ? 0 : stars;
  const set = (r: number) => {
    albumRatings[key] = r;
    const uses = albumUses[source];
    if (uses) uses[id] = { ...(uses[id] ?? NOT_USED), rating: r };
  };
  set(now);
  navigator.vibrate?.(8);
  try {
    await setAlbumRating(source, id, now);
  } catch (e) {
    set(was);
    toast.fail(e, "The rating didn't change");
  }
}

/** An artist's picture on this device: kept as an album's cover is (lib/localCovers), under an id no album has. */
export const artistPictureId = (name: string) => `artist:${name}`;

export const lossless = (a: ListedAlbum) => a.format === 'FLAC';
/** "FLAC", "MP3 256 kbps", "Mixed formats". */
export const quality = (a: ListedAlbum) =>
  [a.format ?? 'Mixed formats', lossless(a) || a.bitrate_kbps === null ? null : `${a.bitrate_kbps} kbps`].filter(Boolean).join(' ');
/** Lossy below 320 kbps somewhere. */
export const weak = (a: ListedAlbum) => !lossless(a) && a.bitrate_kbps !== null && a.bitrate_kbps < 320;
/** When an album was added: "14 Oct" (with the year when it is not this one), or as View's Dates say, how long ago
 *  ("today", "3 days ago", "2 months ago"), in the browser's language. */
export function addedOn(t: number) {
  if (prefs.relativeDates) return ago(t);
  const d = new Date(t * 1000);
  const year = d.getFullYear() === new Date().getFullYear() ? undefined : 'numeric';
  return d.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year });
}

const relative = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
/** How long ago `t` (epoch seconds) was, in the largest unit that fits: days in the first week, then weeks, months,
 *  years. Days are calendar days here (late yesterday is "yesterday", even an hour ago). */
function ago(t: number) {
  const midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((midnight(new Date()) - midnight(new Date(t * 1000))) / 86_400_000);
  if (days < 7) return relative.format(-days, 'day');
  if (days < 30) return relative.format(-Math.floor(days / 7), 'week');
  if (days < 365) return relative.format(-Math.floor(days / 30.44), 'month');
  return relative.format(-Math.floor(days / 365.25), 'year');
}
