// Whoever is signed in (decision 0051, several users): their name, whether they administer this server, what they may
// do, their devices and sign-ins. Read at start (App) and again when the account part changes something. A server that
// answers no /api/me (one from before several users, or sign-in switched off) has one user, its administrator. Until
// the answer comes, nothing of the administrators' shows: a user never sees it flash.
import { ApiError, getMe, type Me, type Permissions } from './api';
import { loadCapabilities } from './capabilities.svelte';

const ALL: Permissions = { offline: true, playlists: true, share: true, links: true, request: true, auto_approve: true, manage_requests: true };

export const me = $state<Me & { known: boolean }>({ known: false, name: null, admin: false, permissions: ALL });

export async function loadMe() {
  try {
    Object.assign(me, { app_password: undefined, devices: undefined, sessions: undefined }, await getMe(), { known: true });
    // Signed in (or again): what the server can do, read with the account (its list is a signed-in one).
    void loadCapabilities();
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) Object.assign(me, { known: true, name: null, admin: true, permissions: ALL });
  }
}
