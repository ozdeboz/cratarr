// Albums not in the library found on Bandcamp (0.9.5, the owner 2026-10-25: "the not in your library section should
// remember Bandcamp URLs and display the appropriate icons on the covers as we do elsewhere"): their pages by release
// group, as the server answers them (it keeps every find a month) and as this device found them since; and Play on
// Bandcamp, as Discover's cards have it (decision 0071), one place for every cover that plays one.
import { ApiError, previewBandcamp, previewBandcampArtist } from './api';
import { albumIdOf } from './bandcampPreview';
import type { LocalTrack } from './localPlayer.svelte';
import { playQueue } from './localPlayer.svelte';
import { port } from './player/port.svelte';
import { can } from './capabilities.svelte';
import { toast } from './toast.svelte';

const pages = $state<Record<string, string>>({});
/** An album's page on Bandcamp by its release group, where one is known. */
export const bandcampOf = (release_group: string, told?: string | null) => told ?? pages[release_group] ?? null;
/** The release group a Bandcamp album (its id, lib/bandcampPreview albumIdOf) was found for, where it was. */
export const releaseGroupOfBandcamp = (album: string) => Object.entries(pages).find(([, p]) => albumIdOf(p) === album)?.[0] ?? null;
export function rememberBandcamp(release_group: string, page: string) {
  pages[release_group] = page;
}

/** What plays (its key) while its tracks are asked for: its cover's button is busy. */
export const bandcampPlay = $state({ key: null as string | null });
/** Why a Bandcamp album last failed to play, by its key (its play button a red "!"); gone once it plays. */
export const bandcampFailed = $state<Record<string, string>>({});
/** Heard in Cratarr's own player, here or on the player this page drives, as a queue of its own; never a play. Not
 *  on the server's own outputs (they play the library's files). */
export async function playOnBandcamp(key: string, page: string, title: string) {
  if (!port().previews) return toast.show({ text: 'A preview plays on this device or on a Cratarr player.', tone: 'error' });
  bandcampPlay.key = key;
  try {
    const p = await previewBandcamp(page);
    playQueue(
      p.tracks.map((t) => ({ ...t, addable: false })),
      0,
      p.album || title,
    );
    delete bandcampFailed[key];
  } catch (e) {
    bandcampFailed[key] = e instanceof Error && e.message ? e.message : 'It could not be played.';
  } finally {
    bandcampPlay.key = null;
  }
}

/** Everything an artist has on Bandcamp as one queue of previews, newest first (New to you's Listen; the owner,
 *  2026-10-27), from their own page where known (`own`); where the server cannot list an artist's (an older one), the
 *  album shown (`page`) alone. */
export async function playArtistOnBandcamp(key: string, artist: string, own: string | null, page: string | null) {
  if (!port().previews) return toast.show({ text: 'A preview plays on this device or on a Cratarr player.', tone: 'error' });
  bandcampPlay.key = key;
  try {
    let tracks: LocalTrack[] = [];
    // Not asked where the server's list (0.11) says it cannot list an artist's and the album shown can stand in.
    if (can('bandcamp_artist') !== false || !page)
      try {
        tracks = (await previewBandcampArtist(artist, own)).albums.flatMap((a) => a.tracks);
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 404) || !page) throw e;
      }
    if (!tracks.length && page) tracks = (await previewBandcamp(page)).tracks;
    if (!tracks.length) throw new Error('Nothing of theirs on Bandcamp.');
    playQueue(
      tracks.map((t) => ({ ...t, addable: false })),
      0,
      artist,
    );
    delete bandcampFailed[key];
  } catch (e) {
    bandcampFailed[key] = e instanceof Error && e.message ? e.message : 'It could not be played.';
  } finally {
    bandcampPlay.key = null;
  }
}
