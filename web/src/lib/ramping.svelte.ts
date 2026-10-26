/** A start/stop ramp of this device's player under way (lib/localPlayer's Start and stop, or the app's own, which tells
 *  the page): the play/pause button drains its fill top to bottom over a ramp out (the owner, 2026-10-26). `at`: when it
 *  began (a new ramp starts the drain over). */
export const ramping = $state({ dir: null as 'out' | 'in' | null, ms: 0, at: 0, full: 0, ended: 0 });

/** `full`: the ramp's set length, where this one is shorter (a wind-up near the track's start, the owner 2026-10-26:
 *  only as long as there is track before it): the button's fill starts that much of the way. */
export function rampBegins(dir: 'out' | 'in', ms: number, full = ms) {
  // The player's word for a ramp the press began a moment ago (lib/remote playPause): its length, not a new start.
  const same = ramping.dir === dir && performance.now() - ramping.at < CONFIRM_MS;
  ramping.dir = dir;
  ramping.ms = ms;
  ramping.full = Math.max(ms, full);
  if (!same) ramping.at = performance.now();
}
/** How soon after a press the player's own ramp message counts as the same ramp. */
const CONFIRM_MS = 600;

/** A ramp begun at the press (the owner, 2026-10-26: the app's pause could come before its ramp, the colour and the
 *  button then wrong): everything starts with the finger; the player's message refines it. Ends by itself if the
 *  player never ramps. */
export function rampPressed(dir: 'out' | 'in', ms: number, full = ms) {
  rampBegins(dir, ms, full);
  const at = ramping.at;
  setTimeout(() => {
    if (ramping.at === at && ramping.dir === dir) rampEnds();
  }, ms + 1500);
}

/** `ended`: when the last one ended (lib/localPlayer counts the app's position on from there, not through it). */
export function rampEnds() {
  if (ramping.dir !== null) ramping.ended = performance.now();
  ramping.dir = null;
}
