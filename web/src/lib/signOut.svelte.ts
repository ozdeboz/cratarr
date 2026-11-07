// Signing out on this device (decision 0101, points 13–15): what waits of the account goes first, where Cratarr can be
// reached (a few seconds at most); in the app its downloads go too when asked ("Remove this account's downloads"), and
// the app hides the rest of the account's things (lib/app's `sign-out`, answered `signed-out`); the page's own stores
// and the service worker's copies are nobody's until the next sign-in (lib/account); then the server's sign-out ends
// the session. A browser goes on to Cratarr's sign-in page; the app to its own page, nobody's, where Local and the servers
// added with nobody signed in still play, and Cratarr's mode asks for the sign-in (lib/app).
// Only Cratarr can end a session: its cookie is out of the page's reach, and in a browser nothing else ends it. So the page
// has to hear that it did, which a form's post cannot, its answer being the browser's. One that does not reach it is owed
// (lib/account), told at the next chance: at an open it comes before all else, or the account is taken up again, live.
import { signOutBegun, signOutOwed, signOutTold, whenLeaving } from './account';
import { inApp, onApp, toApp } from './app.svelte';

/** Waits for `p`, `ms` at most. */
const within = (p: Promise<unknown>, ms: number) => Promise.race([p.catch(() => {}), new Promise((done) => setTimeout(done, ms))]);

let answered: (() => void) | null = null;
onApp('signed-out', () => answered?.());

/** Whether Cratarr took the sign-out: its answer sends one on to the sign-in page, which `manual` leaves unfollowed. */
async function told(): Promise<boolean> {
  try {
    const r = await fetch('/signout', { method: 'POST', redirect: 'manual' });
    return r.ok || r.type === 'opaqueredirect';
  } catch {
    return false;
  }
}

export async function signOut(removeDownloads: boolean) {
  await within(Promise.all([...whenLeaving].map((send) => send())), 5_000);
  if (inApp) {
    const done = new Promise<void>((resolve) => (answered = resolve));
    toApp({ type: 'sign-out', remove: removeDownloads });
    await within(done, 20_000);
    answered = null;
  }
  signOutBegun();
  const ended = (await within(told(), 10_000)) === true;
  // The app ends both addresses' sessions itself at its next load, told or not (Sessions).
  if (ended || inApp) signOutTold();
  // Not told, the page's own: its sign-in page cannot be reached, and it opens as nobody.
  location.assign(ended && !inApp ? '/signin' : '/');
}

let settling = false;
/** A sign-out owed from before, told now if Cratarr can be reached; then on, as a sign-out goes on. */
export async function settleSignOut() {
  if (!signOutOwed() || settling) return;
  settling = true;
  const ended = await told();
  settling = false;
  if (!ended) return;
  signOutTold();
  location.assign(inApp ? '/' : '/signin');
}
if (typeof window !== 'undefined') {
  addEventListener('online', () => void settleSignOut());
  setInterval(() => void settleSignOut(), 30_000);
}
