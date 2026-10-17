// A new Cratarr goes live while a page is open: the page still runs the code it loaded (the tablet's page ran preview 45
// an hour after 47 was live, 2026-10-17). It looks now and then whether the build it runs is still the one served, and
// when it is not, reloads itself at a moment nothing is lost: nothing playing in this page, nothing being typed.

import { local } from './localPlayer.svelte';
import { playsHere } from './output.svelte';

/** This page's build: its script's name (`index-<hash>.js`); none in development. */
const own =
  typeof document === 'undefined'
    ? null
    : ([...document.querySelectorAll<HTMLScriptElement>('script[type=module][src]')].map((s) => s.src.split('/').pop() ?? '').find((n) => n.startsWith('index-')) ?? null);

/** How often it looks, at most. */
const EVERY = 5 * 60_000;
let looked = 0;
let stale = false;

async function look(now = false) {
  if (!own || stale || (!now && Date.now() - looked < EVERY)) return;
  looked = Date.now();
  try {
    const page = await (await fetch('/', { cache: 'no-store' })).text();
    const served = page.match(/assets\/(index-[\w-]+\.js)/)?.[1];
    if (served && served !== own) stale = true;
  } catch {
    // Out of reach: the next look.
  }
}

/** Nothing would be lost: no music playing in this page, no field being typed in. */
function quiet() {
  if (playsHere() && local.index >= 0 && !local.paused) return false;
  const a = document.activeElement;
  return !(a instanceof HTMLInputElement || a instanceof HTMLTextAreaElement || (a instanceof HTMLElement && a.isContentEditable));
}

async function tick(now = false) {
  await look(now);
  if (stale && quiet()) location.reload();
}

/** Starts looking (main.ts): every minute, and at once when the page comes back into view or online. */
export function watchForUpdates() {
  if (!own) return;
  setInterval(() => void tick(), 60_000);
  document.addEventListener('visibilitychange', () => void tick(document.visibilityState === 'visible'));
  addEventListener('online', () => void tick(true));
}
