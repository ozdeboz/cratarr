// Whether Cratarr answers. Away from home the proxy in front of it refuses with 403 (Cratarr itself never does); a
// stopped Cratarr gets the proxy's empty 502 to 504; without a network the request fails. All three are out of reach:
// the pages say so in one line, the pollers ask less often, and the app opens on its downloads (decision 0033, step 4).
// Without a session away from home, Cratarr asks for its sign-in instead: the page goes to the sign-in page.

export const reach = $state({
  /** The last request did not reach Cratarr. */
  away: false,
  /** Cratarr answered at least once since the page opened. */
  answered: false,
  /** What the pages show came from their last copies (public/sw.js: Cratarr out of reach; the owner, 2026-10-26):
   *  offline, as last seen, until Cratarr answers again. */
  kept: false,
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
  tell(false);
  for (const f of whenReached) f();
}
/** An answer from its last copy (the service worker's X-Cratarr-Kept): out of reach, shown as last seen. */
export function keptAnswer() {
  if (!keptMeansAway(lastReached, Date.now())) return;
  reach.away = true;
  reach.kept = true;
  tell(true);
}

export function unreached() {
  reach.away = true;
  tell(true);
}

if (typeof window !== 'undefined') {
  // No network at all: out of reach at once, before any request finds out (the app says it too: lib/app's `network`).
  addEventListener('offline', unreached);
  if (navigator.onLine === false) reach.away = true;
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

/** Whether a refusal holds the background requests back; how long still (ms). */
export const refusedFor = () => Math.max(0, refusedUntil - Date.now());

export function refused() {
  refusedUntil = Date.now() + REFUSED_MS;
  reach.away = true;
}

/** Whether an answer is Cratarr asking for its sign-in (decision 0039: away from home, no session, or one that ended). */
export const asksSignIn = (res: Response) => res.status === 401 && /^Cookie\b.*realm="Cratarr"/i.test(res.headers.get('WWW-Authenticate') ?? '');

let leaving = false;

/** To Cratarr's sign-in page, once; it comes back here after (the #route too). The background requests wait meanwhile. */
export function toSignIn() {
  if (leaving) return;
  leaving = true;
  refusedUntil = Date.now() + REFUSED_MS;
  location.assign(`/signin?next=${encodeURIComponent(location.pathname + location.search + location.hash)}`);
}
