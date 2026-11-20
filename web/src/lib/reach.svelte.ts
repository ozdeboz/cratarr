// Whether Cratarr answers. Away from home the proxy in front of it refuses with a bare 403 (Cratarr's own says why in its
// words, and is an answer); a stopped Cratarr gets the proxy's empty 502 to 504; without a network the request fails. All
// three are out of reach: the pages say so in one line, the pollers ask less often, and the app opens on its downloads
// (decision 0033, step 4).
// Without a session away from home, Cratarr asks for its sign-in instead: the page goes to the sign-in page.
import { accountForgotten, pageGoes, signOutOwed } from './account';

export const reach = $state({
  /** The last request did not reach Cratarr. */
  away: false,
  /** Cratarr answered at least once since the page opened. */
  answered: false,
  /** What the pages show came from their last copies (public/sw.js: Cratarr out of reach; the owner, 2026-10-26):
   *  offline, as last seen, until Cratarr answers again. */
  kept: false,
  /** The device says it has no network at all: the browser's word, or the app's (lib/away's `network`). */
  noNetwork: false,
  /** The app says it is on mobile data with Sync's Mobile data set to None (CU12, CU13): reads come from the device. */
  mobileOff: false,
});

/** A status with no words from Cratarr in it that means it was not reached, rather than that it refused. */
export const notReached = (status: number) => status === 403 || status === 502 || status === 503 || status === 504;

/** The service worker told when reach changes (public/sw.js): out of reach, it answers from the last copies at once. */
let told: boolean | null = null;
function tell(away: boolean) {
  if (told === away || typeof navigator === 'undefined') return;
  told = away;
  navigator.serviceWorker?.controller?.postMessage({ type: 'reach', away });
}

/** Called each time Cratarr answers (lib/later sends what waited). */
export const whenReached = new Set<() => void>();

/** Called each time a request finds Cratarr out of reach (lib/localPlayer takes up the queue kept on this device). */
export const whenAway = new Set<() => void>();

/** When Cratarr last answered live (ms). */
let lastReached = 0;
/** A slow answer from the last copy this soon after a live one is a slow answer, not out of reach (0.12.3; the owner:
 *  Home flickered: each request past the service worker's wait flipped the page away and the next one back). */
export const KEPT_GRACE_MS = 10_000;
/** Whether an answer from the last copy at `now` means out of reach, the last live answer at `last`. */
export const keptMeansAway = (last: number, now: number) => now - last > KEPT_GRACE_MS;

export function reached() {
  lastReached = Date.now();
  reach.away = false;
  reach.answered = true;
  reach.kept = false;
  reach.noNetwork = false;
  tell(false);
  for (const f of whenReached) f();
}
/** An answer from its last copy (the service worker's X-Cratarr-Kept): out of reach, shown as last seen. */
export function keptAnswer() {
  if (!keptMeansAway(lastReached, Date.now())) return;
  reach.away = true;
  reach.kept = true;
  tell(true);
  for (const f of whenAway) f();
}

export function unreached() {
  reach.away = true;
  tell(true);
  for (const f of whenAway) f();
}

/** Called each time the device says it has a network again. */
export const whenNetwork = new Set<() => void>();

/** The device's own word on its network: with none, out of reach at once. `mobileOff`: the app's, on mobile data with Mobile data
 *  set to None (CU13), where it says so (a browser's word leaves it): what reads depend on is told, as they move to the device. */
export function networkSaid(online: boolean, mobileOff?: boolean) {
  reach.noNetwork = !online;
  if (mobileOff !== undefined) reach.mobileOff = mobileOff;
  if (!online) return unreached();
  for (const f of whenNetwork) f();
}

if (typeof window !== 'undefined') {
  // No network at all: out of reach at once, before any request finds out (the app says it too: lib/app's `network`).
  addEventListener('offline', () => networkSaid(false));
  // Waking (RC01): the network back, or the page shown again, looks for Cratarr once, past any hold.
  addEventListener('online', () => {
    networkSaid(true);
    void wake();
  });
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) void wake();
  });
  if (navigator.onLine === false) reach.away = reach.noNetwork = true;
  // A read of the service worker's reached Cratarr again.
  navigator.serviceWorker?.addEventListener('message', (e) => {
    if (e.data?.type === 'reached' && reach.away) {
      told = false;
      reached();
    }
  });
}

/** After the proxy refused (403 without Cratarr's words: its intrusion blocker banned this address; 401: not signed
 *  in), this long without any background request or reconnect: knocking on through a ban keeps it alive (2026-10-16:
 *  the 403s the polling met while banned got the owner's phone banned again, for hours). */
const REFUSED_MS = 5 * 60_000;
let refusedUntil = 0;

/** Whether a refusal holds the background requests back; how long still (ms). A sign-out owed holds them too (lib/account):
 *  nothing is asked under a session that is over. */
export const refusedFor = () => (signOutOwed() ? REFUSED_MS : Math.max(0, refusedUntil - Date.now()));

export function refused() {
  refusedUntil = Date.now() + REFUSED_MS;
  reach.away = true;
}

/** Why the background requests are held now, in words for the console; null where nothing holds them. */
export function heldWhy(): string | null {
  if (signOutOwed()) return 'a sign-out is owed, until Cratarr hears it';
  const left = refusedUntil - Date.now();
  return left > 0 ? `refused (401 or 403), for ${Math.ceil(left / 1000)} s more` : null;
}
/** The hold said last: its end, or Infinity for a sign-out owed. */
let saidHold: number | null = null;
/** Says in the console why background requests are held, once a hold (a renewed one is another): the desktop shell's log keeps
 *  it (RC01: "unreachable" for hours, and nothing said why). */
export function sayHeld() {
  const hold = signOutOwed() ? Infinity : refusedUntil;
  if (hold === saidHold) return;
  const why = heldWhy();
  if (!why) return;
  saidHold = hold;
  console.warn(`Cratarr: background requests held back: ${why}`);
}

/** How long a look for Cratarr waits for its answer. */
const LOOK_MS = 5_000;

/** Looks for Cratarr, past any copy (public/sw.js leaves /api/health to the network): whether it answered. Not while a refusal
 *  holds the background requests back: this would be one more knock. */
export async function lookForCratarr(): Promise<boolean> {
  if (refusedFor() > 0) return false;
  try {
    const res = await fetch('/api/health', { cache: 'no-store', signal: AbortSignal.timeout(LOOK_MS) });
    if (notReached(res.status)) return false;
    reached();
    return true;
  } catch {
    return false;
  }
}

/** A wake's looks at least this far apart: a page shown again and again knocks once a minute at most, never through a ban. */
const WAKE_APART_MS = 60_000;
let wokeAt = -Infinity;
/** On waking (the network back, the page shown again): one look for Cratarr past any hold (RC01: after a sleep, 0.16.2's
 *  desktop app said "unreachable" for hours while Cratarr was well). Its answer lifts a refusal's hold, as Cratarr answering is
 *  no ban; the proxy's bare refusal, or none, leaves it. Whether Cratarr answered. */
export async function wake(now = Date.now()): Promise<boolean> {
  if (now - wokeAt < WAKE_APART_MS) return false;
  wokeAt = now;
  try {
    const res = await fetch('/api/health', { cache: 'no-store', signal: AbortSignal.timeout(LOOK_MS) });
    if (notReached(res.status)) return false;
    refusedUntil = 0;
    reached();
    return true;
  } catch {
    return false;
  }
}

/** Whether an answer is Cratarr asking for its sign-in (decision 0039: away from home, no session, or one that ended). */
export const asksSignIn = (res: Response) => res.status === 401 && /^Cookie\b.*realm="Cratarr"/i.test(res.headers.get('WWW-Authenticate') ?? '');

let leaving = false;
/** Set by lib/app: whether the sign-in waits, `go` called once it may (decision 0101: in the app, nobody signed in
 *  still plays Local and other servers' modes, so the sign-in comes only in Cratarr's). */
let holding: ((go: () => void) => boolean) | null = null;
export const onSignInAsked = (f: (go: () => void) => boolean) => (holding = f);

/** To Cratarr's sign-in page, once; it comes back here after (the #route too). The background requests wait meanwhile. */
export function toSignIn() {
  if (leaving) return;
  refusedUntil = Date.now() + REFUSED_MS;
  // Cratarr says nobody is signed in here (a session that ended, not one out of reach): the name kept is not to open the
  // stores for whoever signs in next.
  accountForgotten();
  if (holding?.(signIn)) return;
  signIn();
}
function signIn() {
  if (leaving) return;
  leaving = true;
  pageGoes();
  location.assign(`/signin?next=${encodeURIComponent(location.pathname + location.search + location.hash)}`);
}
