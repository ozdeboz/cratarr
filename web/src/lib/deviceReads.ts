// What a read answers from the device: the index's rows in the shapes Cratarr gives the pages (`GET /api/albums`, a record's view), so
// that a page cannot tell one from the other. Pure. What Cratarr says and the index does not hold is left as a list with nothing to
// say would leave it: what the viewer may do with a copy (nothing, with Cratarr away), a record's own tags, plays, words, verdicts.
// The rows say which copy plays by their order (`copy_for` stays the server's); lib/copiesRule reads the rest.
import type { AlbumCopy, AlbumsSource, AlbumsView, AlbumView, LibraryInfo, ListedAlbum, PlaylistRow } from './api';

/** Rows as the rules crate writes them (server/rules/src/index/read.rs). */
export type LibraryRow = { id: number; key: string; name: string; kind: 'main' | 'own' | 'incoming'; order: number; colour: string | null; owner: string | null };
export type CopyRow = { library: string; dir: string; format: string | null; kbps: number | null; bits: number | null; rate: number | null; size: number | null; added: number | null; tracks: number };
export type RecordRow = {
  id: string;
  title: string;
  sort: string;
  artist: string;
  artists: string[];
  year: number | null;
  original_year: number | null;
  added: number | null;
  kind: string | null;
  secondary: string[];
  genres: string[];
  whole_tracks: number | null;
  tracks: number;
  seconds: number | null;
  copies: CopyRow[];
};
export type TrackRow = {
  id: string;
  disc: number | null;
  number: number | null;
  title: string;
  artist: string | null;
  line: string;
  seconds: number | null;
  year: number | null;
  bpm: number | null;
  born: number | null;
};
export type ListRows = { rev: number; libraries: LibraryRow[]; records: RecordRow[] };
export type RecordRows = { record: RecordRow; tracks: TrackRow[] };

/** Cratarr's own library, the one an index holds, as the pages name it. */
export const OWN = 'cratarr';
/** One's own music, as the lists name it for its owner: an index has one name for everyone (app::device_index). */
const YOUR_MUSIC = 'Your music';

function infoOf(l: LibraryRow, account: string | null): LibraryInfo {
  const mine = l.owner !== null && l.owner === account;
  return { id: l.key, name: mine ? YOUR_MUSIC : l.name, kind: l.kind, fetcher: null, owner: l.owner, mine, colour: l.colour, quality: 'none', seen: true };
}

const copyOf = (c: CopyRow, at: number): AlbumCopy => ({
  library: c.library,
  format: c.format,
  kbps: c.kbps,
  bits: c.bits,
  rate: c.rate,
  deletable: false,
  keepable: false,
  keep_to: null,
  lacks: [],
  plays: at === 0,
  added: c.added,
  size: c.size,
  tracks: c.tracks,
  job: null,
});

function listedOf(r: RecordRow): ListedAlbum {
  const plays = r.copies[0];
  return {
    id: r.id,
    title: r.title,
    artist: r.artist || null,
    year: r.year,
    added: r.added,
    path: plays ? `${plays.library.replace(/\/+$/, '')}/${plays.dir}` : null,
    tracks: r.tracks,
    format: plays?.format ?? null,
    bitrate_kbps: plays?.kbps ?? null,
    duration_secs: r.seconds,
    tags: [],
    copies: r.copies.map(copyOf),
    ...(r.artists.length > 1 ? { artists: r.artists } : {}),
    type: r.kind,
    secondary: r.secondary,
    ...(r.whole_tracks === null ? {} : { whole_tracks: r.whole_tracks }),
  };
}

/** The Library's list, as of `at` (ms), for `account`. */
export function listView(rows: ListRows, account: string | null, at: number): AlbumsView {
  const libraries = [...rows.libraries].sort((a, b) => a.order - b.order);
  const source: AlbumsSource = {
    id: OWN,
    label: 'Cratarr',
    supported: true,
    albums: rows.records.map(listedOf),
    error: null,
    as_of: Math.floor(at / 1000),
    libraries: libraries.map((l) => l.key),
    library_info: libraries.map((l) => infoOf(l, account)),
    verdicts: {},
    lacks: {},
    waiting: {},
    deletable: [],
    keepable: [],
    keep_to: {},
  };
  return { sources: [source], trash_days: 0, music: libraries.find((l) => l.kind === 'main')?.name ?? null };
}

const rowOf = (album: string) => (t: TrackRow): PlaylistRow => ({
  entry: t.line,
  file: t.line.slice(t.line.lastIndexOf('/') + 1),
  id: t.id,
  name: t.title,
  artist: t.artist,
  album,
  duration_secs: t.seconds,
  missing: false,
});

/** A record's view, its rows in the order they play. */
export function recordView({ record, tracks }: RecordRows): AlbumView {
  const plays = record.copies[0];
  return {
    source: OWN,
    id: record.id,
    title: record.title,
    artist: record.artist || null,
    year: record.year,
    genres: record.genres,
    format: plays?.format ?? null,
    bitrate_kbps: plays?.kbps ?? null,
    rows: tracks.map(rowOf(record.title)),
    ...(record.added === null ? {} : { added: record.added }),
  };
}

/** The number in a record's id; null for an id that is none of the index's. */
export const recordNumber = (id: string) => (/^al-\d+$/.test(id) ? Number(id.slice(3)) : null);
