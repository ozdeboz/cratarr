// In the app: the covers of the albums downloaded on this device, kept by the service worker (sw.js, "keep-covers")
// so they show away from home, where only downloads play (decision 0033, step 4). Told again when the downloads change;
// the worker drops the covers no longer listed and fetches the new ones.
import { untrack } from 'svelte';
import { askDownloads, downloads, inApp } from './app.svelte';
import { albumOfDownload, library, readLibrary } from './albums.svelte';
import { serverCoverUrl } from './api';

/** Started by the shell; returns the stop function. */
export function keepCoversAway(): (() => void) | undefined {
  if (!inApp || !('serviceWorker' in navigator)) return;
  askDownloads();
  let told = '';
  return $effect.root(() => {
    $effect(() => {
      if (!downloads.known) return;
      // A download without its album finds it in the Library's list (lib/albums.svelte's albumOfDownload).
      if (!library.view && downloads.tracks.some((t) => !t.album_id)) untrack(() => readLibrary(false)).catch(() => {});
      const urls = [
        ...new Set(
          downloads.tracks.flatMap((t) => {
            const album = albumOfDownload(t);
            return album ? [serverCoverUrl(t.source, album)] : [];
          }),
        ),
      ].sort();
      // A download's progress is told too, every step: the list goes again only when it changed.
      const key = urls.join('\n');
      if (key === told) return;
      told = key;
      navigator.serviceWorker.ready.then((r) => r.active?.postMessage({ type: 'keep-covers', urls }));
    });
  });
}
