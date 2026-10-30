// A crate as records (0.10; the owner, 2026-10-30): its tracks, which its rules picked and ranked, grouped by the album
// each is of, in the crate's order of each album's first track by default, or by an album order (artist, title, year,
// added). A record says how much of the album the crate holds ("3 of 10 tracks"). Records or tracks is the crate's own
// choice, remembered; the crates that are albums already (Home's New for you and Grave digging) start as records.
// Pure: rows in, records out.
import type { OrderField } from './pageView.svelte';

export type CrateView = 'records' | 'tracks';
/** The crates made of whole albums: shown as records unless chosen otherwise. */
export const RECORD_CRATES: readonly string[] = ['home:new', 'home:graves'];
/** How crate `id` shows: as chosen for it, else as its kind starts. */
export const crateViewOf = (id: string, kept: Readonly<Record<string, CrateView>>): CrateView => kept[id] ?? (RECORD_CRATES.includes(id) ? 'records' : 'tracks');

export type RecordOrder = 'own' | 'artist' | 'title' | 'year' | 'added' | 'tracks' | 'length';
export const RECORD_ORDERS: OrderField<RecordOrder>[] = [
  { value: 'own', label: 'Crate order' },
  { value: 'artist', label: 'Artist' },
  { value: 'title', label: 'Title' },
  { value: 'year', label: 'Year' },
  { value: 'added', label: 'Added', most: true },
  // As the Library's records table orders (the owner, 2026-10-30: one table head): the crate's tracks of it, its length.
  { value: 'tracks', label: 'Tracks', most: true },
  { value: 'length', label: 'Length', most: true },
];

/** A record's columns as a table (the Library's records', lib/columns `albums`, as far as a crate's record has them):
 *  each column's order where it has one. */
export const RECORD_COLUMNS: Readonly<Record<string, RecordOrder>> = { year: 'year', tracks: 'tracks', length: 'length', added: 'added' };

/** What a record is made from: a crate's row, as the panel has it. */
export type RecordRow = { album_id?: string; album: string | null; artist: string | null; album_artist?: string; year?: number; added?: number; album_tracks?: number; duration_secs?: number | null };
export type CrateRecord = {
  /** The album's id, else its name and album artist (a row without an album id). */
  key: string;
  albumId: string | null;
  title: string;
  artist: string | null;
  year: number | null;
  added: number | null;
  /** Its rows' places in the crate, in the crate's order. */
  at: number[];
  /** How many tracks its album has, as its rows say (Cratarr's own library's); null: not said. */
  of: number | null;
  /** How long the crate's tracks of it play together (seconds; those that say). */
  secs: number;
};

const loose = (s: string | null | undefined) => (s ?? '').normalize('NFKD').replace(/\p{M}/gu, '').toLowerCase().trim();

/** The rows (each with its place in the crate, `i`) grouped by album, in the order the first of each comes. */
export function recordsOf<R extends RecordRow>(rows: readonly { r: R; i: number }[]): CrateRecord[] {
  const out = new Map<string, CrateRecord>();
  for (const { r, i } of rows) {
    const key = r.album_id ? `id:${r.album_id}` : `name:${loose(r.album)}\n${loose(r.album_artist ?? r.artist)}`;
    let rec = out.get(key);
    if (!rec) {
      rec = { key, albumId: r.album_id ?? null, title: r.album ?? '', artist: r.album_artist ?? r.artist, year: r.year ?? null, added: r.added ?? null, at: [], of: null, secs: 0 };
      out.set(key, rec);
    }
    rec.at.push(i);
    if (r.duration_secs) rec.secs += r.duration_secs;
    if (rec.of === null && typeof r.album_tracks === 'number' && r.album_tracks > 0) rec.of = r.album_tracks;
    if (r.year != null && (rec.year === null || r.year < rec.year)) rec.year = r.year;
    if (r.added != null && (rec.added === null || r.added > rec.added)) rec.added = r.added;
  }
  return [...out.values()];
}

const words = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base', numeric: true });
const byNumber = (x: number | null, y: number | null) => (x === y ? 0 : x === null ? -1 : y === null ? 1 : x - y);

/** Records in `key`'s order, turned round with `down`; equals keep the crate's order. */
export function orderRecords(recs: readonly CrateRecord[], key: RecordOrder, down: boolean): CrateRecord[] {
  const list = [...recs];
  if (key === 'own') return down ? list.reverse() : list;
  const way = down ? -1 : 1;
  const cmp = (a: CrateRecord, b: CrateRecord) =>
    key === 'artist' ? words(a.artist ?? '', b.artist ?? '') || words(a.title, b.title)
    : key === 'title' ? words(a.title, b.title)
    : key === 'year' ? byNumber(a.year, b.year)
    : key === 'tracks' ? a.at.length - b.at.length
    : key === 'length' ? a.secs - b.secs
    : byNumber(a.added, b.added);
  return list.sort((a, b) => way * cmp(a, b));
}

/** What a record holds of its album: "3 of 10 tracks" where the album's own count is known, else "3 tracks". */
export const holdsLine = (n: number, of: number | null | undefined) =>
  of != null && of >= n ? `${n} of ${of} ${of === 1 ? 'track' : 'tracks'}` : `${n} ${n === 1 ? 'track' : 'tracks'}`;

/** A record's value in column `id` (lib/columns `albums`): the crate's tracks of it ("3/10" where the album's count is
 *  known), their length, its year, when it came. `clock` writes seconds as a time, `day` a date. */
export function recordCell(rec: CrateRecord, id: string, of: number | null, clock: (s: number) => string, day: (t: number) => string): string {
  if (id === 'year') return rec.year ? String(rec.year) : '';
  if (id === 'tracks') return of != null && of >= rec.at.length ? `${rec.at.length}/${of}` : String(rec.at.length);
  if (id === 'length') return rec.secs ? clock(rec.secs) : '';
  if (id === 'added') return rec.added ? day(rec.added) : '';
  return '';
}
