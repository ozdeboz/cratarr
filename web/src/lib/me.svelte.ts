// Whoever is signed in (decision 0051, several users): their name, whether they administer this server, what they may
// do, their devices and sign-ins. Read at start (App) and again when the account part changes something. A server that
// answers no /api/me (one from before several users, or sign-in switched off) has one user, its administrator. Until
// the answer comes, nothing of the administrators' shows: a user never sees it flash. The app is told whose its things
// are now (decision 0101), at once.
import { accountLearnt, signOutOwed } from './account';
import { ApiError, getMe, type Me, type Permissions } from './api';
import { toApp } from './app.svelte';
import { loadCapabilities } from './capabilities.svelte';
import { reach } from './reach.svelte';
import { settleSignOut } from './signOut.svelte';

const ALL: Permissions = { offline: true, playlists: true, share: true, links: true, request: true, auto_approve: true, manage_requests: true };

export const me = $state<Me & { known: boolean }>({ known: false, name: null, admin: false, permissions: ALL });

export async function loadMe() {
  // A session whose sign-out did not get through stands, and /api/me would take it up: Cratarr is told that first.
  if (signOutOwed()) return settleSignOut();
  try {
    const answer = await getMe();
    // A sign-out began while it was asked: it speaks of a session on its way out, and the app would take her back on it.
    if (signOutOwed()) return;
    Object.assign(me, { app_password: undefined, devices: undefined, sessions: undefined }, answer, { known: true });
    // The worker's last copy says who was, and shows as last seen: only Cratarr says who is.
    if (!reach.kept) {
      accountLearnt(me.name);
      toApp({ type: 'account', name: me.name ?? '' });
    }
    // Signed in (or again): what the server can do, read with the account (its list is a signed-in one).
    void loadCapabilities();
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) {
      Object.assign(me, { known: true, name: null, admin: true, permissions: ALL });
      accountLearnt(null);
      toApp({ type: 'account', name: '' });
    }
  }
}
