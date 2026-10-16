// What the album lists (Incoming, Collection) share: playing an album, and how its facts read.
import {
  albumView,
  getAlbumRating,
  getAlbums,
  getAlbumUses,
  playAlbum,
  queueTracks,
  setAlbumRating,
  type AlbumsView,
  type AlbumUse,
  type ListedAlbum,
} from './api';
import { player, refresh, untilNewTrack } from './live.svelte';
import { enqueue, type LocalTrack } from './localPlayer.svelte';
import { isLocal, playAlbumHere, playPlaylistFrom } from './output.svelte';
import { artistSubject, viewOver } from './panel.svelte';
import { prefs } from './prefs.svelte';
import { toast } from './toast.svelte';

/** The Library's list (GET /api/albums) as last read: the page shows it, and the album panel's menu finds the album's
 *  library there (what it may do with it). */
export const library = $state({ view: null as AlbumsView | null });
let reading: Promise<void> | null = null;
/** Reads it (`fresh`: past the service worker's copy); one read at a time. */
export function readLibrary(fresh: boolean): Promise<void> {
  reading ??= getAlbums(prefs.source || undefined, fresh)
    .then((v) => void (library.view = v))
    .finally(() => (reading = null));
  return reading;
}

/** The album being started (its Play buttons wait meanwhile). */
export const albumPlay = $state({ starting: null as string | null });

/** Plays an album: here when the output is this browser (decision 0022), else on the server's player. */
export async function playListedAlbum(source: string, a: ListedAlbum) {
  if (albumPlay.starting) return;
  albumPlay.starting = a.id;
  try {
    if (await playAlbumHere(source, a.id, a.title)) return;
    await playAlbum(source, a.id);
    // Show the album's track once it plays, not the states on the way.
    await untilNewTrack();
  } catch (e) {
    toast.fail(e, 'Could not start playing');
  } finally {
    albumPlay.starting = null;
  }
}

/** Several albums' tracks, one album after another, in each album's order (an album's select bar). */
export async function tracksOfAlbums(list: { source: string; a: ListedAlbum }[]): Promise<LocalTrack[]> {
  const views = await Promise.all(list.map(({ source, a }) => albumView(source, a.id).then((r) => ({ r, a }))));
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
  else if (isLocal()) enqueue(tracks, mode);
  else {
    await queueTracks(
      tracks.map((t) => t.id),
      mode,
      source,
    );
    setTimeout(refresh, 700);
  }
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
export function goToArtist(source: string, name: string) {
  if (player.full) player.full = false;
  viewOver(artistSubject(source, name));
}

// How the owner used the albums (decided 2026-10-16): plays, last played and rating, per server, for the Library's
// orders (Plays, Last played, Rating) and filters (Not played yet, 4 stars and up). Read when one of those is chosen,
// and again each time (plays change).
export const albumUses = $state<Record<string, Record<string, AlbumUse>>>({});
const NOT_USED: AlbumUse = { plays: 0, rating: 0 };
export const useOf = (source: string, id: string): AlbumUse => albumUses[source]?.[id] ?? NOT_USED;
export async function loadAlbumUses(source: string) {
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
