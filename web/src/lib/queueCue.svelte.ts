// Tracks put in the queue from outside the queue sheet (Play next, Add to queue: a track's menu, a list, a crate, an
// album), told here so the mini-player's queue button can answer (the owner, 2026-10-24): it shows Lucide's list-start
// a moment for Play next, list-end for Add to queue, its queue icon again after. Told where each of them reaches the
// queue: the server's add and play calls (lib/api) and this browser's own queue (lib/localPlayer's enqueue).

export const queueCue = $state<{ how: 'next' | 'last' | null; n: number }>({ how: null, n: 0 });

/** Tracks went in: right after the playing one (`next`), or after those added before (`last`). */
export function cue(how: 'next' | 'last') {
  queueCue.how = how;
  queueCue.n++;
}
