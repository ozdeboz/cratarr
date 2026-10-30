// Hold to hear (decided 2026-10-16): a queue row's cover held plays its track for as long as the finger stays, over what
// plays, which pauses and goes on where it was when the finger lifts. It starts a third of the way in, where a track
// has usually begun (30 s in when its length is unknown). This page's own stream plays it in an <audio> of its own,
// from the stream the player uses (/api/stream, its `from_ms`); in the app it would be the app's (not yet).
import { local, togglePlay } from './localPlayer.svelte';
import { playsHere } from './output.svelte';

export type Previewed = { source: string; id: string; duration_secs?: number | null };

/** The track being heard, while a cover is held. */
export const preview = $state({ id: null as string | null });

let el: HTMLAudioElement | null = null;
/** What played paused for it: goes on afterwards. */
let resume = false;

/** Whether a held cover can be heard where the music plays now. */
export const canPreview = () => playsHere();

export function startPreview(t: Previewed) {
  if (preview.id || !canPreview()) return;
  preview.id = t.id;
  navigator.vibrate?.(15);
  const from = t.duration_secs ? t.duration_secs / 3 : 30;
  resume = local.index >= 0 && !local.paused;
  if (resume) togglePlay();
  el = new Audio(`/api/stream/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}?from_ms=${Math.floor(from * 1000)}`);
  el.play().catch(() => endPreview());
}

export function endPreview() {
  if (!preview.id) return;
  preview.id = null;
  el?.pause();
  el?.removeAttribute('src');
  el = null;
  if (resume && local.paused) togglePlay();
  resume = false;
}
