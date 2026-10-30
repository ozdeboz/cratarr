// The sleep timer (the player's moon, top right): the music pauses at a set time or when the playing track ends, and
// the timer clears. The player that plays keeps the deadline, since a page's timers are throttled or frozen behind a
// locked screen: this page when its own stream plays, else the app's player or the Cratarr player this page drives,
// through localPlayer's sleep command (decision 0031). What every page shows comes from that player's reports.
import { inApp } from './app.svelte';
import { port } from './player/port.svelte';
import { handSleep, local, togglePlay, type Sleep } from './localPlayer.svelte';
import { drivenName } from './playOn.svelte';

/** Whether the music plays in another player (the app's, a Cratarr player), which then keeps the timer. */
const handed = () => inApp || drivenName() !== null;

// This page's own timer, for its own stream.
const own = $state<{ sleep: Sleep | null }>({ sleep: null });
let timer: ReturnType<typeof setTimeout> | undefined;

/** The timer as the playing side has it (a deadline, or the end of the playing track); null where there is none. */
export function sleep(): Sleep | null {
  if (!port().sleeps) return null;
  return handed() ? (local.sleep ?? null) : own.sleep;
}

// The clock the remaining time is shown by: once a second, only while a deadline runs.
const clock = $state({ now: Date.now() });
$effect.root(() => {
  $effect(() => {
    if (sleep()?.atMs == null) return;
    clock.now = Date.now();
    const id = setInterval(() => (clock.now = Date.now()), 1000);
    return () => clearInterval(id);
  });
});

/** What is left, for the moon's label and the sheet: "24 min", "End of track"; '' without a timer. */
export function sleepLeft(): string {
  const s = sleep();
  if (!s) return '';
  if (s.endOfTrack) return 'End of track';
  return `${Math.max(1, Math.ceil(((s.atMs ?? clock.now) - clock.now) / 60_000))} min`;
}

/** Sets the timer (`minutes` from now, or the end of the playing track), or clears it (null). */
export function setSleep(to: { minutes: number } | 'endOfTrack' | null) {
  const s: Sleep | null = to === null ? null : to === 'endOfTrack' ? { atMs: null, endOfTrack: true } : { atMs: Date.now() + to.minutes * 60_000, endOfTrack: false };
  if (handed()) {
    handSleep(s);
    return;
  }
  own.sleep = s;
  arm();
}

/** Cratarr's sleep command, for this page while it is a Play On target (lib/playOnTarget): its own timer. */
export function setOwnSleep(s: Sleep | null) {
  own.sleep = s;
  arm();
}

function arm() {
  clearTimeout(timer);
  const at = own.sleep?.atMs;
  if (at != null) timer = setTimeout(check, Math.max(0, at - Date.now()));
}

/** Pauses this page's stream once the deadline has passed (a timer runs late, never early). */
function check() {
  const at = own.sleep?.atMs;
  if (at == null) return;
  if (Date.now() >= at) fire();
  else arm();
}

function fire() {
  own.sleep = null;
  clearTimeout(timer);
  if (!local.paused) togglePlay();
}

// A page brought back from the background checks at once: its timer may have been held up there.
if (typeof document !== 'undefined') document.addEventListener('visibilitychange', () => !document.hidden && check());

// The end of the track: the stream pauses when it moves on to the next one (or is within a quarter second of the end),
// and the next track stays where it began, to be played from its start.
$effect.root(() => {
  let index = local.index;
  $effect(() => {
    const now = local.index;
    if (own.sleep?.endOfTrack && !handed()) {
      const nearEnd = local.duration !== null && local.duration - local.position < 0.25;
      if (now !== index || nearEnd) fire();
    }
    index = now;
  });
});
