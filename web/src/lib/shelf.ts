// The Downloads shelf (decision 0085, step 2; the owner, 2026-10-30: the page's own store in 0.12, the Rust core's from
// 0.13): what is downloaded on this device kept with its details and a small cover, so Downloads shows, searches and
// orders it with no network. The records and the rules over them, pure; lib/shelfStore keeps them in IndexedDB.
//
// Two places hold downloads: the app's (its Downloads, over the bridge, `app`) and an installed page's own (lib/offline,
// `browser`). Each says what it holds; the shelf follows it: a track it no longer holds goes. What a download says of a
// track (its name, artist, album) is kept at once; the rest (year, length, genres, its number on the album) comes from
// its album's view the next time Cratarr answers, one album at a time, and again after REFILL_MS.
import { matches, words } from './search';

export type ShelfWhere = 'app' | 'browser';

/** A downloaded track as the shelf keeps it. `filled`: when its album's details were read (epoch ms; 0: not yet). */
export type ShelfTrack = {
  key: string;
  where: ShelfWhere;
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  album_artist: string | null;
  year: number | null;
  duration_secs: number | null;
  genres: string[];
  disc: number | null;
  track: number | null;
  /** When the shelf first saw it (epoch ms): the order downloads came in. */
  added: number;
  filled: number;
};

/** What a place says of a track it holds. */
export type Held = {
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  duration_secs?: number | null;
};

/** An album's view as the shelf reads it (lib/api AlbumView, the fields it uses). */
export type AlbumDetails = {
  title: string;
  artist: string | null;
  year: number | null;
  genres: string[];
  rows: { id: string | null; name: string | null; artist: string | null; duration_secs: number | null; disc?: number; track?: number; year?: number; album_artist?: string }[];
};

/** Details are read again after this long (an album retagged, a genre added). */
export const REFILL_MS = 30 * 24 * 3600_000;

export const keyOf = (where: ShelfWhere, t: { source: string; id: string }) => `${where}:${t.source}/${t.id}`;
export const coverKey = (source: string, album: string) => `${source}/${album}`;

/** A track held, as a record: one kept already keeps its details; what the place says now wins for what it says. */
export function recordOf(where: ShelfWhere, t: Held, was: ShelfTrack | undefined, now: number): ShelfTrack {
  return {
    key: keyOf(where, t),
    where,
    source: t.source,
    id: t.id,
    name: t.name,
    artist: t.artist,
    album: t.album,
    album_id: t.album_id,
    album_artist: was?.album_artist ?? null,
    year: was?.year ?? null,
    duration_secs: t.duration_secs ?? was?.duration_secs ?? null,
    genres: was?.genres ?? [],
    disc: was?.disc ?? null,
    track: was?.track ?? null,
    added: was?.added ?? now,
    filled: was && was.album_id === t.album_id ? was.filled : 0,
  };
}

const same = (a: ShelfTrack, b: ShelfTrack) => JSON.stringify(a) === JSON.stringify(b);

/** What a place holds now against what the shelf keeps of it: the records to write (new or changed) and the keys to take
 *  away (no longer held there). Another place's records are left alone. */
export function plan(kept: readonly ShelfTrack[], where: ShelfWhere, held: readonly Held[], now: number): { put: ShelfTrack[]; remove: string[] } {
  const mine = new Map(kept.filter((r) => r.where === where).map((r) => [r.key, r]));
  const put: ShelfTrack[] = [];
  const seen = new Set<string>();
  for (const t of held) {
    const key = keyOf(where, t);
    if (seen.has(key)) continue;
    seen.add(key);
    const was = mine.get(key);
    const next = recordOf(where, t, was, now);
    if (!was || !same(was, next)) put.push(next);
  }
  const remove = [...mine.keys()].filter((k) => !seen.has(k));
  return { put, remove };
}

/** The albums whose details are still to be read (or read too long ago), the oldest-added first: one read each. Tracks
 *  without an album id have none to read. */
export function toFill(kept: readonly ShelfTrack[], now: number): { source: string; album_id: string }[] {
  const by = new Map<string, { source: string; album_id: string; added: number }>();
  for (const r of kept) {
    if (!r.album_id || (r.filled && now - r.filled < REFILL_MS)) continue;
    const k = coverKey(r.source, r.album_id);
    const was = by.get(k);
    if (!was || r.added < was.added) by.set(k, { source: r.source, album_id: r.album_id, added: r.added });
  }
  return [...by.values()].sort((a, b) => a.added - b.added).map(({ source, album_id }) => ({ source, album_id }));
}

/** The album's records with its details read in: each track found among its rows by id; one not found is marked read
 *  all the same (with the album's own details), so it is not asked again until REFILL_MS. */
export function filled(kept: readonly ShelfTrack[], source: string, album_id: string, view: AlbumDetails, now: number): ShelfTrack[] {
  const rows = new Map(view.rows.filter((r) => r.id).map((r) => [r.id as string, r]));
  return kept
    .filter((r) => r.source === source && r.album_id === album_id)
    .map((r) => {
      const row = rows.get(r.id);
      return {
        ...r,
        album: r.album ?? view.title,
        album_artist: row?.album_artist ?? view.artist ?? r.album_artist,
        year: row?.year ?? view.year ?? r.year,
        duration_secs: row?.duration_secs ?? r.duration_secs,
        genres: view.genres.length ? [...view.genres] : r.genres,
        disc: row?.disc ?? r.disc,
        track: row?.track ?? r.track,
        filled: now,
      };
    });
}

/** Whether a record answers the words typed (lib/search, as every search reads): its name, artist, album, album
 *  artist, genres and year. */
export const shelfMatches = (r: ShelfTrack, query: string) =>
  matches(words(query), r.name, r.artist, r.album, r.album_artist, r.genres.join(' '), r.year ? String(r.year) : null);

/** An album as the shelf groups it: its details as its tracks have them, its tracks in their album order. */
export type ShelfAlbum = {
  source: string;
  album_id: string | null;
  title: string;
  artist: string | null;
  year: number | null;
  genres: string[];
  tracks: ShelfTrack[];
  duration_secs: number;
  added: number;
};

/** The records by album (a track without an album id its own), each album's tracks by disc and number. */
export function albumsOf(kept: readonly ShelfTrack[]): ShelfAlbum[] {
  const by = new Map<string, ShelfAlbum>();
  for (const r of kept) {
    const k = r.album_id ? `${r.where}:${coverKey(r.source, r.album_id)}` : r.key;
    const a = by.get(k) ?? {
      source: r.source,
      album_id: r.album_id,
      title: r.album ?? r.name,
      artist: r.album_artist ?? r.artist,
      year: r.year,
      genres: r.genres,
      tracks: [],
      duration_secs: 0,
      added: r.added,
    };
    a.tracks.push(r);
    a.duration_secs += r.duration_secs ?? 0;
    a.added = Math.max(a.added, r.added);
    a.year ??= r.year;
    if (!a.genres.length) a.genres = r.genres;
    by.set(k, a);
  }
  const n = (x: number | null) => x ?? Number.MAX_SAFE_INTEGER;
  for (const a of by.values()) a.tracks.sort((x, y) => n(x.disc) - n(y.disc) || n(x.track) - n(y.track) || x.name.localeCompare(y.name));
  return [...by.values()];
}

/** An album answers the words typed when it or one of its tracks does. */
export const albumMatches = (a: ShelfAlbum, query: string) =>
  !words(query).length || matches(words(query), a.title, a.artist, a.genres.join(' '), a.year ? String(a.year) : null) || a.tracks.some((t) => shelfMatches(t, query));

export type ShelfOrder = 'added' | 'title' | 'artist' | 'year';

/** Albums in an order: the newest download first, or by title, artist (then year) or year (the newest first). */
export function orderAlbums(albums: readonly ShelfAlbum[], by: ShelfOrder): ShelfAlbum[] {
  const t = (s: string | null) => (s ?? '').toLocaleLowerCase();
  const cmp: Record<ShelfOrder, (a: ShelfAlbum, b: ShelfAlbum) => number> = {
    added: (a, b) => b.added - a.added,
    title: (a, b) => t(a.title).localeCompare(t(b.title)),
    artist: (a, b) => t(a.artist).localeCompare(t(b.artist)) || (a.year ?? 0) - (b.year ?? 0) || t(a.title).localeCompare(t(b.title)),
    year: (a, b) => (b.year ?? -1) - (a.year ?? -1) || t(a.title).localeCompare(t(b.title)),
  };
  return [...albums].sort(cmp[by]);
}

/** The cover's longest side kept on the shelf (px): a row's cover, a few KB. */
export const COVER_PX = 160;

/** The size a cover is drawn at to fit COVER_PX on its longest side, never larger than it is. */
export function coverSize(w: number, h: number): { w: number; h: number } {
  const s = Math.min(1, COVER_PX / Math.max(w, h, 1));
  return { w: Math.max(1, Math.round(w * s)), h: Math.max(1, Math.round(h * s)) };
}
