// The Want path (W12, R22, C49b): every Want sends the record it is of (`al-<n>`), else what the card knows it by, its release
// group and its Bandcamp page, with its names; the server finds the record by them. A track is wanted as its album, by the album's
// own artist. Whether an album was asked for comes from the server: the account's requests by record, else by release group;
// never by names. Pure.
import { requestOpen, type Asked } from './requestState';

/** What a card knows of the album it wants. */
export type Wantable = {
  record?: string | null;
  release_group?: string | null;
  /** Its album page on Bandcamp. */
  page?: string | null;
  artist: string;
  album: string;
  year?: number | null;
  cover?: string | null;
};
/** What `POST /api/requests/want` takes (`WantAlbum`). */
export type WantBody = { record?: string; release_group?: string; page?: string; artist: string; album: string; year?: number; cover?: string };

/** A Want: by its record where it has one (its names said too, the server goes by the record); else by its group and page. */
export function wantOf(x: Wantable): WantBody {
  if (x.record) return { record: x.record, artist: x.artist, album: x.album };
  return {
    ...(x.release_group ? { release_group: x.release_group } : {}),
    ...(x.page ? { page: x.page } : {}),
    artist: x.artist,
    album: x.album,
    ...(x.year ? { year: x.year } : {}),
    ...(x.cover ? { cover: x.cover } : {}),
  };
}

/** A request of the account as the Requests page lists it: its record (the catalogue's number) and its release group. */
type Mine = Asked & { record?: number | null; release_group?: string | null };

/** Whether the account has an open request of this album: one naming its record, else (no record holds it yet) its group. */
export function askedFor(mine: readonly Mine[], x: { record?: string | null; release_group?: string | null }): boolean {
  const of = (m: Mine) =>
    x.record ? m.record !== null && m.record !== undefined && `al-${m.record}` === x.record : !!x.release_group && m.release_group?.toLowerCase() === x.release_group.toLowerCase();
  return mine.some((m) => requestOpen(m) && of(m));
}

/** A Want that failed, in words: out of reach it needs Cratarr, as a record not had does; else the server's words. */
export const wantFailed = (words: string | null, unreached: boolean) => (unreached ? 'Wanting needs Cratarr.' : (words ?? 'Could not ask for it'));
