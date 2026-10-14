// What the album lists (Incoming, Collection) share: playing an album, and how its facts read.
import { playAlbum, type ListedAlbum } from './api';
import { untilNewTrack } from './live.svelte';
import { playAlbumHere } from './output.svelte';
import { toast } from './toast.svelte';

/** The album being started (its Play buttons wait meanwhile). */
export const albumPlay = $state({ starting: null as string | null });

/** Plays an album: here when the output is this browser (decision 0022), else on the server's player. */
export async function playListedAlbum(source: string, a: ListedAlbum) {
  if (albumPlay.starting) return;
  albumPlay.starting = a.id;
  try {
    if (await playAlbumHere(source, a.id, a.title)) return;
    await playAlbum(source, a.id);
    // Show the album's track once it plays, not the states on the way.
    await untilNewTrack();
  } catch (e) {
    toast.fail(e, 'Could not start playing');
  } finally {
    albumPlay.starting = null;
  }
}

export const lossless = (a: ListedAlbum) => a.format === 'FLAC';
/** "FLAC", "MP3 256 kbps", "Mixed formats". */
export const quality = (a: ListedAlbum) =>
  [a.format ?? 'Mixed formats', lossless(a) || a.bitrate_kbps === null ? null : `${a.bitrate_kbps} kbps`].filter(Boolean).join(' ');
/** Lossy below 320 kbps somewhere. */
export const weak = (a: ListedAlbum) => !lossless(a) && a.bitrate_kbps !== null && a.bitrate_kbps < 320;
export const addedOn = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
