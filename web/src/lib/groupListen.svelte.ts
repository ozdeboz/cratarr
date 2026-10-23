// Sync play in a browser (decision 0037, amended 2026-10-23: the owner wanted the ThinkBook in the tablet's group): a
// page follows a group as a Kodi does. It plays the group's sound as one long WAV (`/api/groups/{leader}/listen.wav`,
// sent a buffer after its stamps, less its speaker delay), and says every few seconds what it plays there and when, on
// Cratarr's time (a clock exchange with `/api/time`), so Cratarr skips or pads to keep it in step with the others.
import { pageVolume } from './localPlayer.svelte';

const HEARD_MS = 2_000;
const CLOCK_MS = 60_000;
/** Its sound lost (an error, the stream ended): opened again after this long. */
const REOPEN_MS = 2_000;

export const listening = $state({ leader: null as string | null, playing: false });

let el: HTMLAudioElement | null = null;
let player = '';
let heardTimer: ReturnType<typeof setInterval> | undefined;
let clockTimer: ReturnType<typeof setInterval> | undefined;
let reopenTimer: ReturnType<typeof setTimeout> | undefined;
/** Cratarr's time less this browser's (ms), from the exchange with the shortest round trip. */
let offset = 0;

async function measureClock() {
  let best = Infinity;
  for (let i = 0; i < 5; i++) {
    try {
      const t0 = Date.now();
      const r = (await (await fetch('/api/time', { cache: 'no-store' })).json()) as { now_ms: number };
      const t1 = Date.now();
      if (t1 - t0 < best) {
        best = t1 - t0;
        offset = r.now_ms - (t0 + t1) / 2;
      }
    } catch {
      /* the next round */
    }
  }
}

function heard() {
  if (!el || el.paused || !listening.leader) return;
  const body = { time_ms: el.currentTime * 1000, at_ms: Date.now() + offset };
  fetch(`/api/players/${encodeURIComponent(player)}/heard`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  }).catch(() => {
    /* the next one */
  });
}

function volume() {
  if (el) el.volume = (pageVolume.level / 100) * (pageVolume.ducked ? 0.25 : 1);
}

/** The sound let go, its stream closed. */
function close() {
  const a = el;
  el = null;
  if (!a) return;
  a.pause();
  a.removeAttribute('src');
  a.load();
}

/** Its group's sound, opened (again); whether the browser let it sound (it may want a tap first). */
async function open(): Promise<boolean> {
  clearTimeout(reopenTimer);
  close();
  if (!listening.leader) return false;
  const a = new Audio(`/api/groups/${encodeURIComponent(listening.leader)}/listen.wav?player=${encodeURIComponent(player)}`);
  el = a;
  volume();
  const lost = () => {
    if (a !== el) return;
    listening.playing = false;
    clearTimeout(reopenTimer);
    reopenTimer = setTimeout(() => listening.leader && open(), REOPEN_MS);
  };
  a.addEventListener('playing', () => a === el && (listening.playing = true));
  a.addEventListener('pause', () => a === el && (listening.playing = false));
  a.addEventListener('error', lost);
  a.addEventListener('ended', lost);
  try {
    await a.play();
    return true;
  } catch {
    return false;
  }
}

/** Follows `leader`'s group as player `name`: its sound plays here. Whether it sounds already (else: a tap). */
export async function followGroup(leader: string, name: string): Promise<boolean> {
  if (listening.leader === leader && player === name && el && !el.paused) return true;
  listening.leader = leader;
  player = name;
  clearInterval(heardTimer);
  clearInterval(clockTimer);
  await measureClock();
  heardTimer = setInterval(heard, HEARD_MS);
  clockTimer = setInterval(measureClock, CLOCK_MS);
  return open();
}

/** After the tap a browser asks for. */
export const resumeGroup = () => open();

/** Out of the group: its sound stops. */
export function leaveGroup() {
  listening.leader = null;
  listening.playing = false;
  clearInterval(heardTimer);
  clearInterval(clockTimer);
  clearTimeout(reopenTimer);
  close();
}

if (typeof window !== 'undefined') {
  $effect.root(() => {
    $effect(() => {
      void [pageVolume.level, pageVolume.ducked];
      volume();
    });
  });
}
