// Where the playing track is now, to the frame: the one clock what moves with the music draws by (the visuals' Music,
// the player's band glass; the owner, 2026-10-29: "engine purity", where the glass ran a clock of its own at plain
// speed). This device's player says it to the frame (lib/localPlayer positionNow), through a wind, a brake and a
// rewind as it goes. Another player reports it now and then, through the server: between reports it moves on with the
// frame clock, and it is eased toward each report rather than set to it, since a jump back or forth would be a beat
// played twice or skipped; a report far off (a seek) is taken as it is.
import { positionNow } from './localPlayer.svelte';
import { isLocal } from './output.svelte';
import { paused, playing } from './remote.svelte';

let clock = { key: '', pos: 0, at: 0, heard: Number.NaN };

/** The place of track `key` at `at` (performance.now()), `on` while it plays. */
export function placeAt(key: string, on: boolean, at: number): number {
  if (isLocal()) return positionNow();
  const heard = playing()?.progress?.position_secs ?? 0;
  if (clock.key !== key) clock = { key, pos: heard, at, heard };
  if (on) clock.pos += (at - clock.at) / 1000;
  clock.at = at;
  if (heard !== clock.heard) {
    const off = heard - clock.pos;
    clock.pos += Math.abs(off) > 0.3 ? off : off / 2;
    clock.heard = heard;
  }
  return clock.pos;
}

/** The playing track's place now (s); 0 with none. */
export function placeNow(at = performance.now()): number {
  const np = playing();
  return np ? placeAt(`${np.source}/${np.id}`, !paused(), at) : 0;
}
