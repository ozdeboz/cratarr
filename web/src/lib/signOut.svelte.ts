// Signing out on this device (decision 0101, points 13–15): what waits of the account goes first, where Cratarr can be
// reached (a few seconds at most); in the app its downloads go too when asked ("Remove this account's downloads"), and
// the app hides the rest of the account's things (lib/app's `sign-out`, answered `signed-out`); the page's own stores
// and the service worker's copies are nobody's until the next sign-in (lib/account); then the server's sign-out ends
// the session. A browser goes on to Cratarr's sign-in page (its form, posted); the app to its own page, nobody's, where
// Local and the servers added with nobody signed in still play, and Cratarr's mode asks for the sign-in (lib/app).
import { accountForgotten } from './account';
import { inApp, onApp, toApp } from './app.svelte';
import { sendLater } from './later.svelte';

/** Waits for `p`, `ms` at most. */
const within = (p: Promise<unknown>, ms: number) => Promise.race([p.catch(() => {}), new Promise((done) => setTimeout(done, ms))]);

let answered: (() => void) | null = null;
onApp('signed-out', () => answered?.());

export async function signOut(form: HTMLFormElement, removeDownloads: boolean) {
  await within(sendLater(), 5_000);
  if (!inApp) {
    accountForgotten();
    form.requestSubmit();
    return;
  }
  const done = new Promise<void>((resolve) => (answered = resolve));
  toApp({ type: 'sign-out', remove: removeDownloads });
  await within(done, 20_000);
  answered = null;
  accountForgotten();
  await within(fetch('/signout', { method: 'POST' }), 10_000);
  location.assign('/');
}
