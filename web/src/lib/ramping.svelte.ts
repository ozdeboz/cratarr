/** A start/stop ramp of this device's player under way (lib/localPlayer's Start and stop, or the app's own, which tells
 *  the page): the play/pause button drains its fill top to bottom over a ramp out (the owner, 2026-10-26). `at`: when it
 *  began (a new ramp starts the drain over). */
export const ramping = $state({ dir: null as 'out' | 'in' | null, ms: 0, at: 0, full: 0, ended: 0, effect: true });
/** A tape rewind sounds on this device's player (the page's own, lib/localPlayer, or the app's, as it reports it):
 *  the shown position goes back with it (lib/remote), and it is never a ramp of play or pause (the button's fill). */
export const rewindNow = $state({ on: false, asked: false });
/** A hold under way on this page's player (its brake let go included) and the tape's speed now (× the track's): the
 *  waveform zooms on the playhead by it (lib/SeekWave; the owner, 2026-10-28). `landing`: while held, where letting go
 *  now would bring the tape back to its own speed (forwards) or to a stand (backwards), s into the track shown. */
export const holdNow = $state({ on: false, speed: 0, top: 16, landing: null as number | null, landingIndex: null as number | null, dir: 1 as 1 | -1, held: false });
/** The look-ahead's line as last painted (lib/SeekWave): s into which track, and when; lib/localPlayer compares it with
 *  where the tape plays on at its own speed (the owner, 2026-10-28: the instrumentation for the aim). */
export const aimPainted = { pos: null as number | null, track: null as string | null, at: 0 };
/** A wind asked for and not sounding yet (a seek that winds, here or on the player driven): the waveform already goes
 *  with it, never to where it goes (the owner, 2026-10-27: the waveform in sync with the seek). */
let askedTimer: ReturnType<typeof setTimeout> | undefined;
export function windAsked(on: boolean) {
  rewindNow.asked = on;
  clearTimeout(askedTimer);
  if (on) askedTimer = setTimeout(() => (rewindNow.asked = false), 3000);
}
/** The tape deck's direction knob (the owner, 2026-10-27): `want`, where it is set (R forwards, L backwards); `state`,
 *  what the tape does about it (lib/localPlayer's turning: it slows to a stop before it can go the other way). */
export const tapeDir = $state({ want: 'R' as 'L' | 'R', state: 'forward' as 'forward' | 'stopping' | 'backward' | 'returning' });

/** `full`: the ramp's set length, where this one is shorter (a wind-up near the track's start, the owner 2026-10-26:
 *  only as long as there is track before it): the button's fill starts that much of the way. */
/** `effect`: a Start and stop effect (play, pause, stop, a skip), whose way the play button's fill and the waveform's
 *  colour follow; not one of the tape's own (a hold's stop before it winds, a turn of the direction knob, the wind-up
 *  after a wind: the owner, 2026-10-27, only an effect that stops the music greys it). */
export function rampBegins(dir: 'out' | 'in', ms: number, full = ms, effect = true) {
  ramping.effect = effect;
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

/** The play button's fill now (0 empty … 1 full) as a ramp under way draws it (lib/NowPlaying: evenly over its time);
 *  null with none under way. */
export function shownFill(now = performance.now()): number | null {
  if (!ramping.dir) return null;
  const u = Math.min(1, Math.max(0, (now - ramping.at) / Math.max(1, ramping.ms)));
  if (ramping.dir === 'in') {
    const pre = 1 - ramping.ms / Math.max(1, ramping.full);
    return pre + (1 - pre) * u;
  }
  return Math.min(1, ramping.ms / Math.max(1, ramping.full)) * (1 - u);
}
/** A press during the app's ramp turns it round from the fill shown (the owner, 2026-10-28): a start's into a stop from
 *  as full as it got, over that share of the stop (`stopMs`); a stop's into a start over the share left (`startMs`).
 *  The app's own word for the turned ramp, a moment later, refines its length, not its start (rampBegins). */
export function rampTurned(stopMs: number, startMs: number) {
  const f = shownFill();
  if (f === null) return;
  if (ramping.dir === 'in') rampPressed('out', stopMs * f, stopMs);
  else rampPressed('in', startMs * (1 - f), startMs);
}

/** `ended`: when the last one ended (lib/localPlayer counts the app's position on from there, not through it). */
export function rampEnds() {
  if (ramping.dir !== null) ramping.ended = performance.now();
  ramping.dir = null;
}
