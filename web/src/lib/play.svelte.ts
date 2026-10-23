// One way to play anything (decided 2026-10-23; server/src/app/play.rs): the server picks what plays from the whole
// collection, at most 500 tracks, so every surface (a card, a panel, a menu) plays the same. Here (this browser, or one
// of Cratarr's players the page drives: the page holds the queue) the page queues the tracks the server chose; on a
// media server's player the server queues them itself.
import { pickCollection, type Collection, type CrateLook, type Picking, type PlayHow } from './api';
import { hiddenLibraries } from './listFilter.svelte';
import { crates } from './crates.svelte';
import { refresh, untilNewTrack } from './live.svelte';
import { enqueue, playQueue } from './localPlayer.svelte';
import { isLocal } from './output.svelte';
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

/** The tracks the server picks of `what`, without playing them (a radio's seed). */
export const pickOf = (what: Collection, how: PlayHow, source = prefs.source || undefined) => pickCollection(shown(what), how, { source });

/** Plays `what` where the output is: played, shuffled, put next or added to the queue; `name` is the queue's (a crate's
 *  Excludes find their crate by it). `from`: the track (its id) to play first. */
export async function playCollection(what: Collection, how: PlayHow, name: string, from?: string, look?: CrateLook): Promise<void> {
  const source = prefs.source || undefined;
  if (isLocal()) {
    const got = await pickCollection(shown(what, look), how, { from, source });
    if (how === 'next' || how === 'last') return enqueue(got.tracks, how);
    return playQueue(got.tracks, got.index, name);
  }
  await pickCollection(shown(what, look), how, { from, source, on: 'server' });
  // Show the new track once it plays, not the states on the way.
  if (how === 'play' || how === 'shuffle') await untilNewTrack();
  else setTimeout(refresh, 700);
}
