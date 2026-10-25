// An artist's page from Discover (the owner, 2026-10-25: "artist pages should also be visitable so long as artists are
// in our discovery"): an artist the library has nothing of, as the panel shows the library's (`artist:discover/<name>`),
// their albums the suggestions of theirs that play on Bandcamp (each opening as a Bandcamp album), their picture and
// words as New to you has them. Nothing of a server's: no tracks, no heart, no menu of the library's.
import { getDiscover, type ArtistView, type ListedAlbum } from './api';
import { albumIdOf } from './bandcampPreview';
import { markOf } from './requests.svelte';

/** The source an artist from Discover is viewed under. */
export const DISCOVER = 'discover';

/** The Bandcamp album an artist's page from Discover was opened from (the owner, 2026-10-26: a page said "Nothing by
 *  …" for an artist not among today's suggestions): shown among their albums whatever Discover suggests, and its
 *  release group, where known, tells the server whose discography to list (GET /api/artists/missing). */
export const cameFrom: { name: string | null; album: { id: string; title: string; year: number | null; release_group: string | null } | null } = { name: null, album: null };

/** Their page: their suggestions that play on Bandcamp, newest first, as albums; how many studio albums they made. */
export async function discoverArtistView(name: string): Promise<ArtistView> {
  const v = await getDiscover();
  const all = (v.found?.albums ?? []).filter((a) => a.artist.toLowerCase() === name.toLowerCase());
  const theirs = all.filter((a) => a.bandcamp);
  const albums: ListedAlbum[] = theirs
    .sort((a, b) => (b.year ?? 0) - (a.year ?? 0))
    .map((a) => ({
      id: albumIdOf(a.bandcamp!),
      // Asked for: where it stands, on its cover (lib/CoverStatus).
      request: a.request ?? markOf(v.requests?.[a.key]),
      title: a.album,
      artist: a.artist,
      year: a.year,
      added: null,
      path: null,
      tracks: a.track_count ?? 0,
      format: 'MP3',
      bitrate_kbps: 128,
      tags: a.genres ?? [],
    })) as ListedAlbum[];
  // How many studio albums they have made (the owner, 2026-10-25: "the golden standard"), as the look counted; the panel
  // asks the server's beside it where there is none (lib/PlaylistView), not holding the page.
  const studio = all.find((a) => a.artist_albums)?.artist_albums ?? null;
  // The album the page was opened from, where it is not among them.
  const c = cameFrom.name?.toLowerCase() === name.toLowerCase() ? cameFrom.album : null;
  if (c && !albums.some((a) => a.id === c.id)) {
    albums.unshift({ id: c.id, title: c.title, artist: name, year: c.year, added: null, path: null, tracks: 0, format: 'MP3', bitrate_kbps: 128 } as ListedAlbum);
  }
  return { source: DISCOVER, name, albums, appears_on: [], rows: [], studio } as unknown as ArtistView;
}
