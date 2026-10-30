// Where the playing track is now, to the frame: what moves with the music draws by it (the visuals' Music, the player's
// band glass; the owner, 2026-10-29: "engine purity", where the glass ran a clock of its own at plain speed). This
// device's player says it to the frame (lib/localPlayer positionNow), through a wind, a brake and a rewind as it goes;
// another player's place is the one clock's (lib/player/clock), the same anchor the player's bar reads.
import { positionNow } from './localPlayer.svelte';
import { clock, duration, paused, playing } from './remote.svelte';
import { placeOf } from './player/clock';
import { port } from './player/port.svelte';

/** The playing track's place at `at` (performance.now()). `key` and `on` are the caller's view of it; the clock's
 *  anchor knows the track and whether it plays. */
export function placeAt(_key: string, _on: boolean, at: number): number {
  if (port().holdsQueue) return positionNow();
  return placeOf(clock.anchor, at, duration());
}

/** The playing track's place now (s); 0 with none. */
export function placeNow(at = performance.now()): number {
  const np = playing();
  return np ? placeAt(`${np.source}/${np.id}`, !paused(), at) : 0;
}
