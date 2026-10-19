// A new Cratarr goes live while a page is open: the page still runs the code it loaded (the tablet's page ran preview 45
// an hour after 47 was live, 2026-10-17). It looks now and then whether the build it runs is still the one served, and
// when it is not, reloads itself at a moment nothing is lost.
//
// In the app the music is not the page's: the app's own player plays it and goes on through a reload, so music playing
// does not hold the reload there (it did, and the salon tablet, playing all day, ran 63's page under 64 until it was
// reloaded by hand: its visuals were not shared, 2026-10-18). In a browser the page plays the music itself, so there it
// waits until the music is paused. Either way it waits while something is being typed, for half a minute after a touch
// or a key; the visuals, where they are open, open again after it.

import { inApp } from './app.svelte';
import { local } from './localPlayer.svelte';
import { playsHere } from './output.svelte';
import { REOPEN, visuals } from './visuals/shown.svelte';

/** This page's build: its script's name (`index-<hash>.js`); none in development. */
const own =
  typeof document === 'undefined'
    ? null
    : ([...document.querySelectorAll<HTMLScriptElement>('script[type=module][src]')].map((s) => s.src.split('/').pop() ?? '').find((n) => n.startsWith('index-')) ?? null);

/** How often it looks, at most. */
const EVERY = 5 * 60_000;
/** How long after a touch or a key it leaves the page alone (s). */
const IDLE_S = 30;
/** The build this tab last reloaded for (sessionStorage): reloaded once for a build, not again if it still is not it. */
const TRIED = 'cratarr-update-tried';

let looked = 0;
let served: string | null = null;
let touched = 0;

async function look(now = false) {
  if (!own || (!now && Date.now() - looked < EVERY)) return;
  looked = Date.now();
  try {
    const page = await (await fetch('/', { cache: 'no-store' })).text();
    served = page.match(/assets\/(index-[\w-]+\.js)/)?.[1] ?? null;
  } catch {
    // Out of reach: the next look.
  }
}

const session = {
  get: (k: string) => {
    try {
      return sessionStorage.getItem(k);
    } catch {
      return null;
    }
  },
  set: (k: string, v: string) => {
    try {
      sessionStorage.setItem(k, v);
    } catch {
      /* not kept: the visuals stay closed after it */
    }
  },
};

/** Nothing would be lost: no music this page plays itself, nothing typed, nobody at it. */
function quiet() {
  if (!inApp && playsHere() && local.index >= 0 && !local.paused) return false;
  if (Date.now() - touched < IDLE_S * 1000) return false;
  const a = document.activeElement;
  return !(a instanceof HTMLInputElement || a instanceof HTMLTextAreaElement || (a instanceof HTMLElement && a.isContentEditable));
}

async function tick(now = false) {
  await look(now);
  if (!served || served === own || session.get(TRIED) === served || !quiet()) return;
  session.set(TRIED, served);
  if (visuals.open) session.set(REOPEN, '1');
  location.reload();
}

/** Starts looking (main.ts): every minute, and at once when the page comes back into view or online. */
export function watchForUpdates() {
  if (!own) return;
  for (const kind of ['pointerdown', 'keydown', 'wheel'] as const) addEventListener(kind, () => (touched = Date.now()), { capture: true, passive: true });
  setInterval(() => void tick(), 60_000);
  document.addEventListener('visibilitychange', () => void tick(document.visibilityState === 'visible'));
  addEventListener('online', () => void tick(true));
}
