// Editions of one album (F12.1; the owner: "One card, 2 editions"): records the server takes for editions of one album (one
// release group where both are known; else one artist and one title, most tracks paired) show as one card that says how many;
// the album page lists each with its track count and copies, each opening its record. Display only: the records stay two
// (Q8). Pure.
import type { AlbumCopy, LibraryInfo } from './api';

/** A card's mark where it stands for several editions: "2 editions"; none for one. */
export const editionsMark = (n: number | null | undefined): string | null => (n && n >= 2 ? `${n} editions` : null);
/** A row's line with the mark after its parts, as they are joined ("Applescal · 2012 · 2 editions"). */
export const withEditions = (line: string | null | undefined, n: number | null | undefined): string => [line, editionsMark(n)].filter(Boolean).join(' · ');

/** One edition of an album page's record, as its answer lists them (the card's first; the record shown among them). */
export type Edition = { album: string; tracks: number; copies: AlbumCopy[] };
/** An edition's line on the album page: "10 tracks · Music MP3"; `here` the one the page shows (no tap), the others open. */
export type EditionLine = { album: string; text: string; here: boolean };
/** Each edition's line, in the server's order: its tracks, then each copy's library and format. None for one edition. */
export function editionLines(editions: readonly Edition[] | null | undefined, here: string, info: readonly Pick<LibraryInfo, 'id' | 'name'>[] | undefined): EditionLine[] {
  if (!editions || editions.length < 2) return [];
  const copy = (c: Pick<AlbumCopy, 'library' | 'format'>) => [info?.find((l) => l.id === c.library)?.name ?? c.library, c.format].filter(Boolean).join(' ');
  return editions.map((e) => ({
    album: e.album,
    text: [`${e.tracks} ${e.tracks === 1 ? 'track' : 'tracks'}`, e.copies.map(copy).join(', ')].filter(Boolean).join(' · '),
    here: e.album === here,
  }));
}
