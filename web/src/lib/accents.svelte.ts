// Each track's own colour: the accent of its cover (lib/remote's accentOf), for waveforms of several tracks side by side
// (lib/SeekWave's wide view; the owner, 2026-10-29: "colors should stay true to their waveform", none crossing a
// boundary). Asked once a track and kept; a cover is fetched ahead, so a track's colour is there before it shows. A
// track whose cover has none, or does not load, has none here: the caller takes the app's accent.
import { albumCoverUrl, coverUrl } from './api';
import { accentOf } from './remote.svelte';

/** The colours known, by `source/id`: an accent, or null where its cover gave none. */
const known = $state<Record<string, string | null>>({});
const asked = new Set<string>();

/** A track's colour as known now (its cover fetched the first time it is asked for): its cover's accent, else null. */
export function accentFor(t: { source: string; id: string; album_id?: string | null; addable?: boolean }): string | null {
  const key = `${t.source}/${t.id}`;
  if (!asked.has(key)) {
    asked.add(key);
    void (async () => {
      // In the order the player takes its cover (lib/remote's coverSrc), so a track's colour here is the player's own.
      const library = coverUrl(t);
      const album = t.album_id ? albumCoverUrl(t.source, t.album_id) : null;
      for (const src of t.addable === false ? [album, library] : [library, album]) {
        const a = src ? await accentOf(src) : null;
        if (a) return void (known[key] = a);
      }
      known[key] = null;
    })();
  }
  return known[key] ?? null;
}

/** The playing track's colour as the player has it (lib/remote's art): kept as its own, so its waveform keeps it once
 *  it is the track before. */
export function keepAccent(t: { source: string; id: string }, accent: string | null) {
  if (!accent) return;
  const key = `${t.source}/${t.id}`;
  asked.add(key);
  if (known[key] !== accent) known[key] = accent;
}
