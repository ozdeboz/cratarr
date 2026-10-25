// Albums not in the library found on Bandcamp (0.9.5, the owner 2026-10-25: "the not in your library section should
// remember Bandcamp URLs and display the appropriate icons on the covers as we do elsewhere"): their pages by release
// group, as the server answers them (it keeps every find a month) and as this device found them since; and Play on
// Bandcamp, as Discover's cards have it (decision 0071), one place for every cover that plays one.
import { previewBandcamp } from './api';
import { playQueue } from './localPlayer.svelte';
import { isLocal } from './output.svelte';
import { toast } from './toast.svelte';

const pages = $state<Record<string, string>>({});
/** An album's page on Bandcamp by its release group, where one is known. */
export const bandcampOf = (release_group: string, told?: string | null) => told ?? pages[release_group] ?? null;
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
  if (!isLocal()) return toast.show({ text: 'A preview plays on this device or on a Cratarr player.', tone: 'error' });
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
