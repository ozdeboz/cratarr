/** A start/stop ramp of this device's player under way (lib/localPlayer's Start and stop, or the app's own, which tells
 *  the page): the play/pause button drains its fill top to bottom over a ramp out (the owner, 2026-10-26). `at`: when it
 *  began (a new ramp starts the drain over). */
export const ramping = $state({ dir: null as 'out' | 'in' | null, ms: 0, at: 0, full: 0 });

/** `full`: the ramp's set length, where this one is shorter (a wind-up near the track's start, the owner 2026-10-26:
 *  only as long as there is track before it): the button's fill starts that much of the way. */
export function rampBegins(dir: 'out' | 'in', ms: number, full = ms) {
  ramping.dir = dir;
  ramping.ms = ms;
  ramping.full = Math.max(ms, full);
  ramping.at = performance.now();
}

export const rampEnds = () => void (ramping.dir = null);
