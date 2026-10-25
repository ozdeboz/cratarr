// An artist's page from Discover (the owner, 2026-10-25: "artist pages should also be visitable so long as artists are
// in our discovery"): an artist the library has nothing of, as the panel shows the library's (`artist:discover/<name>`),
// their albums the suggestions of theirs that play on Bandcamp (each opening as a Bandcamp album), their picture and
// words as New to you has them. Nothing of a server's: no tracks, no heart, no menu of the library's.
import { getDiscover, type ArtistView, type ListedAlbum } from './api';
import { albumIdOf } from './bandcampPreview';

/** The source an artist from Discover is viewed under. */
export const DISCOVER = 'discover';

/** Their page: their suggestions that play on Bandcamp, newest first, as albums; how much they have released. */
export async function discoverArtistView(name: string): Promise<ArtistView> {
  const v = await getDiscover();
  const all = (v.found?.albums ?? []).filter((a) => a.artist.toLowerCase() === name.toLowerCase());
  const theirs = all.filter((a) => a.bandcamp);
  const albums: ListedAlbum[] = theirs
    .sort((a, b) => (b.year ?? 0) - (a.year ?? 0))
    .map((a) => ({
      id: albumIdOf(a.bandcamp!),
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
  // How much they have released (the owner, 2026-10-25: "how many releases an artist has done"), as the look counted.
  const releases = all.find((a) => a.artist_releases)?.artist_releases ?? null;
  const albumCount = all.find((a) => a.artist_albums)?.artist_albums ?? null;
  return { source: DISCOVER, name, albums, appears_on: [], rows: [], released: { releases, albums: albumCount } } as unknown as ArtistView;
}
