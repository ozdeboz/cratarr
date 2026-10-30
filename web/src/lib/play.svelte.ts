// One way to play anything (decided 2026-10-23; server/src/app/play.rs): the server picks what plays from the whole
// collection, at most 500 tracks, so every surface (a card, a panel, a menu) plays the same. Here (this browser, or one
// of Cratarr's players the page drives: the page holds the queue) the page queues the tracks the server chose; on a
// media server's player the server queues them itself.
import { ApiError, UNREACHED, pickCollection, type Collection, type CrateLook, type Picked, type Picking, type PlayHow } from './api';
import { away, pickHere } from './away.svelte';
import { hiddenLibraries } from './listFilter.svelte';
import { crates } from './crates.svelte';
import { refresh, untilNewTrack } from './live.svelte';
import { port } from './player/port.svelte';
import { prefs } from './prefs.svelte';

/** A crate as its panel and card show it: its two toggles as set here (a tap may be on its way), a crate not listed
 *  here yet as saved; from the libraries switched on; in its panel, as filtered and ordered there (`look`). */
function shown(what: Collection, look: CrateLook = {}): Picking {
  if (what.kind !== 'crate') return what;
  const c = crates.list?.find((x) => x.id === what.id);
  const without = hiddenLibraries();
  return {
    ...what,
    ...(c ? { favourites: c.favourites, ...(c.favourite_albums ? { favourite_albums: c.favourite_albums } : {}) } : {}),
    ...(without ? { without } : {}),
    ...look,
  };
}

/** The server's pick; out of reach (lib/away), what is on this device of it, picked here as the server would. */
async function pickHereOr(what: Picking, how: PlayHow, from: string | undefined, source: string | undefined): Promise<Picked> {
  if (away()) return pickHere(what, how, from, source);
  try {
    return await pickCollection(what, how, { from, source });
  } catch (e) {
    if (e instanceof ApiError && e.message === UNREACHED) return pickHere(what, how, from, source);
    throw e;
  }
}

/** The tracks the server picks of `what`, without playing them (a radio's seed). */
export const pickOf = (what: Collection, how: PlayHow, source = prefs.source || undefined) => pickCollection(shown(what), how, { source });

/** Plays `what` where the output is: played, shuffled, put next or added to the queue; `name` is the queue's (a crate's
 *  Excludes find their crate by it). `from`: the track (its id) to play first. */
export async function playCollection(what: Collection, how: PlayHow, name: string, from?: string, look?: CrateLook): Promise<void> {
  const source = prefs.source || undefined;
  // Who picks differs, not how it plays: where the page holds the queue it queues the server's pick (or, out of reach,
  // its own of what is on this device); the server's player is given the collection and picks for itself.
  if (port().holdsQueue) {
    const got = await pickHereOr(shown(what, look), how, from, source);
    // The server's shuffle is in its pick already: played as it comes, not shuffled again.
    return port().playTracks(got.tracks, how === 'shuffle' ? 'play' : how, name, got.source, got.index);
  }
  await pickCollection(shown(what, look), how, { from, source, on: 'server' });
  // Show the new track once it plays, not the states on the way.
  if (how === 'play' || how === 'shuffle') await untilNewTrack();
  else setTimeout(refresh, 700);
}
