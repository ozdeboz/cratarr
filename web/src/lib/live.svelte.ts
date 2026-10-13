// One poll of /api/state for the whole app: what is playing, and which playlists hold it. The shell polls it and shows
// the now-playing header on every page; pages read `live` instead of polling themselves.
//
// Player commands are shown at once (`expect`): the header shows what the command should lead to until the server
// agrees, says otherwise, or does not move at all. The server always wins: a different outcome is shown quietly, no
// change at all as an error. Playlist taps made meanwhile wait (`queued`) and go to the track they were made on once
// the server confirms it; otherwise they are dropped, never added to another track.
import { ApiError, getState, setMember, type NowPlaying, type State } from './api';
import { prefs } from './prefs.svelte';
import { remember } from './sources.svelte';
import { toast } from './toast.svelte';
import { on, open, close, stream } from './events.svelte';

/** How a server answer relates to the expected result: it, something else, or not yet. */
export type Settled = 'yes' | 'no' | 'wait';

type Expectation = {
  np: NowPlaying;
  settled: (server: NowPlaying | null) => Settled;
  until: number;
  /** No answer in time: show the server's state without an error (Previous may restart the track instead). */
  quiet: boolean;
};
let expecting: Expectation | null = null;
/** The tracks a run of commands passes through, from the one shown before the first: the server may still show any
 *  of them while it catches up (Next pressed twice: the first skip not yet done when the second is shown). */
let passing: string[] = [];

export const live = $state<{
  view: State | null;
  error: string | null;
  /** A player command is shown before the server confirmed it. */
  pending: boolean;
  /** Playlists tapped while pending, for the expected track. */
  queued: string[];
  /** The track shown before the current one (what Previous goes back to). */
  before: NowPlaying | null;
}>({ view: null, error: null, pending: false, queued: [], before: null });

/** The now-playing header full screen (everything but the navigation); using the navigation puts it back. */
export const player = $state({ full: false });

/** Show `np` now as the result of a player command, until the server settles it (or `ms` passes). */
export function expect(np: NowPlaying, settled: (server: NowPlaying | null) => Settled, { ms = 8000, quiet = false } = {}) {
  const shown = live.view?.now_playing ?? null;
  if (shown && shown.id !== np.id) live.before = shown;
  if (!expecting) passing = [];
  if (shown && !passing.includes(shown.id)) passing.push(shown.id);
  const e: Expectation = {
    np,
    // A track along the way is not an answer yet; only the command's own result, or another track, is.
    settled: (s) => (s && s.id !== np.id && passing.includes(s.id) ? 'wait' : settled(s)),
    until: Date.now() + ms,
    quiet,
  };
  expecting = e;
  live.pending = true;
  if (live.view) live.view = { ...live.view, now_playing: np };
  // The live channel only speaks when something changed: when nothing does, ask once the time is up.
  setTimeout(() => expecting === e && refresh(), ms + 100);
}

/** The command failed: back to what the server shows. */
export function expectFailed(text: string) {
  end(`${text}${live.queued.length ? ', so nothing was added' : ''}.`);
  refresh();
}

/** Tap a playlist while a command is pending: it waits for the expected track (a second tap takes it back). */
export function queueTap(playlist: string) {
  live.queued = live.queued.includes(playlist) ? live.queued.filter((p) => p !== playlist) : [...live.queued, playlist];
}

function end(error?: string) {
  expecting = null;
  passing = [];
  live.pending = false;
  live.queued = [];
  if (error) toast.show({ text: error, tone: 'error' });
}

/** The server confirmed the expected track: the waiting taps go to it, each flipping what the server now says. */
async function flush(view: State) {
  const taps = live.queued;
  live.queued = [];
  let last: State | null = null;
  for (const id of taps) {
    const p = view.playlists.find((x) => x.id === id);
    if (!p) continue;
    try {
      last = await setMember(id, !p.member, prefs.source || undefined);
    } catch (e) {
      toast.fail(e, `Could not change ${p.name}`);
    }
  }
  if (last && !expecting) live.view = last;
}

/** Ask the server now (pages call it after an action that changed playlists; the shell's poll while the live channel
 *  is down). */
export async function refresh() {
  try {
    take(await getState(prefs.source || undefined));
  } catch (e) {
    live.error = e instanceof ApiError ? e.message : 'Unexpected error';
  }
}

/** A state from the server, by the live channel or asked for: settles a pending command, then shows it. */
function take(fresh: State) {
  let view = fresh;
  {
    remember(view.sources);
    if (expecting) {
      const e = expecting;
      const verdict = e.settled(view.now_playing);
      if (verdict === 'yes') {
        expecting = null;
        passing = [];
        live.pending = false;
        if (live.queued.length) flush(view);
      } else if (verdict === 'no') {
        const lost = live.queued.length ? `Not added: the player didn't move on to ${e.np.name}.` : undefined;
        end(lost);
      } else if (Date.now() > e.until) {
        end(e.quiet ? undefined : `The player didn't follow${live.queued.length ? ', so nothing was added' : ''}.`);
      } else {
        view = { ...view, now_playing: e.np };
      }
    } else {
      const shown = live.view?.now_playing ?? null;
      if (shown && view.now_playing && shown.id !== view.now_playing.id) live.before = shown;
    }
    live.view = view;
    live.error = null;
  }
}

/** Keep `live` current: from the live channel (events.svelte.ts) while it is open, asking every 2 s (5 s while the
 *  page is hidden) while it is not, and every 30 s regardless. Returns a stop function. */
export function poll(): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout>;
  let last = 0;
  const off = on<State>('state', (view) => {
    last = Date.now();
    take(view);
  });
  open(prefs.source || undefined);
  async function tick() {
    const quiet = Date.now() - last;
    if (!stream.connected || quiet > 30_000) await refresh();
    if (!stopped) timer = setTimeout(tick, stream.connected ? 5000 : document.hidden ? 5000 : 2000);
  }
  tick();
  return () => {
    stopped = true;
    clearTimeout(timer);
    off();
    close();
  };
}
