// What the server can do (lib/capabilityRule): read at the start, again each time the server answers after being out of
// reach (a reconnect) and after signing in, and kept in this browser for offline, so a page opened with no network
// offers what the server last said it could do. `can(name)`: true or false as the list says; null where it decides
// nothing (a server before 0.11), and the caller keeps its own check.
import { ApiError, getCapabilities, tellBatches } from './api';
import { answered, canBy, keptState, NO_CAPABILITIES, type CapabilityState } from './capabilityRule';
import { reach } from './reach.svelte';

const KEY = 'cratarr-capabilities';

const kept = (): CapabilityState => {
  try {
    return keptState(JSON.parse(localStorage.getItem(KEY) ?? 'null'));
  } catch {
    return NO_CAPABILITIES;
  }
};

export const capabilities = $state<CapabilityState & { version: string | null }>({ ...(typeof window === 'undefined' ? NO_CAPABILITIES : kept()), version: null });

function set(s: CapabilityState, version: string | null) {
  Object.assign(capabilities, s, { version });
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    /* for this visit only */
  }
}

/** Asks the server again (at the start, on a reconnect, after signing in). */
export async function loadCapabilities() {
  try {
    const c = await getCapabilities();
    set(answered(capabilities, { names: c.capabilities }), c.version);
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) set(answered(capabilities, 'older'), null);
  }
}

/** Whether the server can do `name` (null: its list decides nothing here, the caller's own check stands). */
export const can = (name: string): boolean | null => canBy(capabilities, name);
/** Offered unless the list says the server cannot (a server before 0.11 offers as before). */
export const offers = (name: string): boolean => can(name) !== false;
/** A track found for an album (Missing tracks, Find a better copy): on Soulseek, or from a file on the server. */
export const findsTracks = (): boolean => offers('track_search') || offers('track_intake');

tellBatches(() => can('batches'));

if (typeof window !== 'undefined') {
  void loadCapabilities();
  $effect.root(() => {
    // Back after being out of reach: asked again, so a server changed meanwhile is read anew.
    let away = reach.away;
    $effect(() => {
      const now = reach.away;
      if (away && !now) void loadCapabilities();
      away = now;
    });
  });
}
