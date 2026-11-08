// What this device answers for itself (PREVIEW2's D18 to D20): crates, Sounds like and what comes next, worked out by the rules crate
// from what is kept (`index::device`), as the server would. In a browser by index.wasm, while reads come from the device; in the app
// the app holds the one copy and the page asks it (android's DeviceAnswers), as the tablet could not hold two. Asked through the door
// (lib/api's `read`): an answer of null goes where any read goes, to a last copy or the one out of reach line.
import { accountKey, whoOf } from './account';
import { inApp } from './app.svelte';
import { openReads } from './deviceIndex.svelte';
import { OWN } from './deviceReads';
import type { IndexEngine } from './indexEngine';
import { readBytes } from './indexStore';
import type { Crate, CrateFavourites, CrateLook, QueueTrack, TempoBands } from './api';

/** Where the app answers. */
const APP_ANSWER = '/__device/answer';

/** What the rest of the page keeps and a device must go by: the plays not told yet, and the tracks that play here. Told by those who
 *  keep them (lib/output, lib/away), so this file reaches for neither. */
export const heldHere = { plays: (): unknown[] => [], tracks: (): string[] => [] };

const made = new WeakMap<IndexEngine, Promise<boolean>>();
let toldPlays = '[]';

/** The device made ready over this engine's index, once: its sounds and the account's own, what is kept of them. */
function ready(e: IndexEngine, who: string): Promise<boolean> {
  let done = made.get(e);
  if (!done) {
    done = Promise.all([readBytes(who, 'sounds'), readBytes(who, 'own')]).then(([sounds, own]) => {
      if (sounds) e.loadSounds(sounds);
      if (own) e.loadOwn(own);
      toldPlays = '[]';
      return e.openDevice();
    });
    made.set(e, done);
  }
  return done;
}

/** The answer to what is asked; null where this device has none. */
export async function deviceAsk<T>(asked: Record<string, unknown>): Promise<T | null> {
  if (inApp) {
    const res = await fetch(`${APP_ANSWER}?ask=${encodeURIComponent(JSON.stringify(asked))}`, { cache: 'no-store' });
    return res.ok ? ((await res.json()) as T) : null;
  }
  const account = accountKey();
  const held = account === null ? null : await openReads();
  if (!held || !(await ready(held.engine, whoOf(account)))) return null;
  const plays = JSON.stringify(heldHere.plays());
  if (plays !== toldPlays) {
    held.engine.ask({ ask: 'plays', plays: heldHere.plays() });
    toldPlays = plays;
  }
  return held.engine.ask<T>(asked);
}

const now = () => Math.floor(Date.now() / 1000);
/** Only Cratarr's own library is held here. */
const ours = (source?: string) => !source || source === OWN;

export const deviceCrates = async (source?: string, without?: string[]) => (ours(source) ? deviceAsk<Crate[]>({ ask: 'crates', without: without ?? [], now: now() }) : null);

export const deviceCrate = <T>(id: string) => deviceAsk<T>({ ask: 'crate', id });

const lookOf = (favourites: CrateFavourites | undefined, albums: CrateFavourites | undefined, look: CrateLook) => ({
  favourites,
  favourite_albums: albums,
  without: look.without ?? [],
  filter: look.filter ?? [],
  order: look.order,
  down: !!look.down,
});

export const deviceCrateRows = async <T>(id: string, offset: number, limit: number, source: string | undefined, favourites: CrateFavourites | undefined, albums: CrateFavourites | undefined, look: CrateLook) =>
  ours(source) ? deviceAsk<T>({ ask: 'rows', id, offset, limit, ...lookOf(favourites, albums, look), now: now() }) : null;

export const deviceCrateTempo = async (id: string, source: string | undefined, favourites: CrateFavourites | undefined, albums: CrateFavourites | undefined, look: CrateLook) =>
  ours(source) ? deviceAsk<TempoBands>({ ask: 'tempo', id, ...lookOf(favourites, albums, look), now: now() }) : null;

/** What comes next among what plays here, with no notice of what cannot. The draw is what `rng` is on the server: this pick's own. */
export const devicePick = (ask: Record<string, unknown>) => deviceAsk<{ tracks: QueueTrack[] }>({ ask: 'pick', ...ask, here: heldHere.tracks(), draw: Math.floor(Math.random() * Number.MAX_SAFE_INTEGER), now: now() });
