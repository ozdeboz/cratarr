// One poll of /api/state for the whole app: what is playing, and which playlists hold it. The shell polls it and shows
// the now-playing header on every page; pages read `live` instead of polling themselves.
import { ApiError, getState, type State } from './api';
import { prefs } from './prefs.svelte';
import { remember } from './sources.svelte';

export const live = $state<{ view: State | null; error: string | null }>({ view: null, error: null });

/** Ask the server now (the shell's poll calls this; pages call it after an action that changed playlists). */
export async function refresh() {
  try {
    live.view = await getState(prefs.source || undefined);
    remember(live.view.sources);
    live.error = null;
  } catch (e) {
    live.error = e instanceof ApiError ? e.message : 'Unexpected error';
  }
}

/** Keep `live` current: every 2 s while the page is visible, every 5 s while hidden. Returns a stop function. */
export function poll(): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout>;
  async function tick() {
    await refresh();
    if (!stopped) timer = setTimeout(tick, document.hidden ? 5000 : 2000);
  }
  tick();
  return () => {
    stopped = true;
    clearTimeout(timer);
  };
}
