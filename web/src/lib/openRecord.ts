// An album not in the library opened (W9, C54): its record's page (`album:cratarr/al-<n>`), wherever it is found (an artist's
// missing albums, Beyond, the search's known elsewhere, Spotify, a label). A row no record holds yet is made one by the server
// from what its sources know it by (`POST /api/records/open`, Q33: kept at open), never by its names; a row known by no
// source opens its Bandcamp album, where it has one. Pure.

/** What a row knows of the album it is. */
export type Openable = {
  /** The record that holds it (`al-<n>`). */
  record?: string | null;
  release_group?: string | null;
  discogs_master?: number | null;
  discogs_release?: number | null;
  title: string;
  artist: string;
  year?: number | null;
  /** Its album page on Bandcamp. */
  bandcamp?: string | null;
};
/** What `POST /api/records/open` takes: what the sources know it by, its names, and the label whose page opened it. */
export type RecordOpenBody = {
  release_group?: string;
  discogs_master?: number;
  discogs_release?: number;
  title: string;
  artist: string;
  year?: number;
  label?: string;
};
export type HowToOpen = { record: string } | { open: RecordOpenBody } | { bandcamp: string } | null;

/** How a row opens: its record; else made one by its references (and tied to `label`, where a label's page opened it); else its
 *  Bandcamp album; else not at all. Names alone never open a record. */
export function howToOpen(x: Openable, label?: string | null): HowToOpen {
  if (x.record) return { record: x.record };
  const refs: Partial<RecordOpenBody> = {
    ...(x.release_group ? { release_group: x.release_group } : {}),
    ...(x.discogs_master ? { discogs_master: x.discogs_master } : {}),
    ...(x.discogs_release ? { discogs_release: x.discogs_release } : {}),
  };
  if (Object.keys(refs).length) return { open: { ...refs, title: x.title, artist: x.artist, ...(x.year ? { year: x.year } : {}), ...(label ? { label } : {}) } };
  return x.bandcamp ? { bandcamp: x.bandcamp } : null;
}
