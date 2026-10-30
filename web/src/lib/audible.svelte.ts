// Until what was started can be heard (the owner, 2026-10-26: Play turns into the ring until the music is audible,
// then everything changes at once): the player on another track than before (or this one again from its start), not
// paused, its position moving. Settles after `ms` regardless (a player that never follows), so nothing waits forever.
import { paused, playing, position } from './remote.svelte';

export function untilAudible(ms = 15_000): Promise<void> {
  const fromId = playing()?.id ?? null;
  const fromPos = position();
  return new Promise((resolve) => {
    let first: number | null = null;
    const t0 = performance.now();
    const check = setInterval(() => {
      const np = playing();
      const pos = position();
      const fresh = !!np && (np.id !== fromId || pos + 1 < fromPos);
      if (fresh && !paused()) {
        // Moving: heard once its position has gone on from where it was first seen.
        if (first === null) first = pos;
        else if (pos > first) return done();
      }
      if (performance.now() - t0 > ms) done();
    }, 80);
    function done() {
      clearInterval(check);
      resolve();
    }
  });
}
