// More from, under a record (MORE; the owner, on production 0.17.0: a record of several artist chips showed More by for the first
// alone): a row per artist chip, in the chips' order, each the artist's other records here, newest first; a record shown under an
// earlier chip is not shown again, and an artist with nothing else here has no row (lib/AlbumMore draws each). Pure, so the tests
// take it as it is.
import type { AlbumChip } from './artistIds';
import { artistCredits } from './artistCredits';

/** A record as the library lists it, as far as More from reads it. */
type Listed = { id: string; title: string; artist: string | null; year: number | null; artists?: string[] | null };

/** Whether `a` is `artist`'s: one of its artists by name, or of its artist line's credits. */
const isBy = (a: Listed, artist: string) => {
  const name = artist.toLowerCase();
  return !!a.artists?.some((x) => x.toLowerCase() === name) || artistCredits(a.artist).includes(name);
};

/** `artist`'s other records among `albums`, newest first: never `album` itself, nor one of an artist `above` (a row before it). */
export function moreFrom<A extends Listed>(albums: readonly A[], album: string, artist: string, above: readonly string[] = []): A[] {
  return albums
    .filter((a) => a.id !== album && isBy(a, artist) && !above.some((b) => isBy(a, b)))
    .sort((a, b) => (b.year ?? 0) - (a.year ?? 0) || a.title.localeCompare(b.title));
}

/** Whose rows come under a record: each chip's artist, in the chips' order, once; a record of one chip (whose label is its whole
 *  credit line), its main artist `main`, as before. */
export function moreArtists(chips: readonly AlbumChip[], main: string | null): string[] {
  if (chips.length < 2) return main ? [main] : [];
  const seen = new Set<string>();
  return chips.flatMap((c) => (seen.has(c.label.toLowerCase()) ? [] : (seen.add(c.label.toLowerCase()), [c.label])));
}
