// One's own settings (decision 0051, several users), kept by the server for each account so they follow its user from
// device to device: the More music sheet's choices, Radio's (by what, how many, leaving favourites out) and Autoplay's
// (its mode and albums, or off), and what makes music alike for both ("Similar by", which the server reads itself for
// each choice). A page that starts with nothing playing takes them (localPlayer `startWith`); every
// change of them is kept (`keep`), a second after the last. A server from before several users keeps none: nothing is
// sent to it again.
import { ApiError, getMySettings, putMySettings } from './api';
import type { AutoplayMode, RadioBy } from './localPlayer.svelte';

export type Choices = {
  /** Radio as it starts when switched on: by what, how many; leaving favourites out (both). */
  radio?: { by: RadioBy; count: number; skip_favourites: boolean };
  /** Autoplay as it starts when switched on, and whether each new queue starts with it on (auto arm; absent: armed, as
   *  before). Null, as kept before 0.9.5: not armed. */
  autoplay?: { mode: AutoplayMode; albums: boolean; arm?: boolean } | null;
  similar_by?: Signal[];
  /** What Radio and Autoplay choose from (the owner, 2026-10-25): Cratarr's own libraries (null: all of them), and
   *  Discover's suggestions on Bandcamp. */
  more_music?: MoreFrom;
  /** Whether the account's plays are kept at all (the owner, 2026-10-25; absent: on). */
  tracking?: boolean;
  /** Discover's picks of the day moved on from (the owner, 2026-10-25: rotated from its menu), the latest last. */
  discover_pick_skip?: string[];
  /** Others may follow this account's playing (0.10; off unless switched on), and whose tracks asked for go into
   *  the queue at once. */
  followable?: boolean;
  auto_accept?: string[];
};
export type MoreFrom = { libraries: string[] | null; bandcamp: boolean };

/** What can make music alike for Radio and Autoplay (the server's one similarity, `similarity.rs`), in the order offered. */
export type Signal = 'sound' | 'artists' | 'genre' | 'style' | 'mood' | 'energy' | 'tempo' | 'danceability' | 'era' | 'playlists' | 'tags';
export const SIGNALS: { id: Signal; label: string }[] = [
  { id: 'sound', label: 'Sound' },
  { id: 'artists', label: 'Artist' },
  { id: 'genre', label: 'Genre' },
  { id: 'style', label: 'Style' },
  { id: 'mood', label: 'Mood' },
  { id: 'energy', label: 'Energy' },
  { id: 'tempo', label: 'Tempo' },
  { id: 'danceability', label: 'Danceability' },
  { id: 'era', label: 'Era' },
  { id: 'playlists', label: 'Playlists' },
  { id: 'tags', label: 'Album tags' },
];

/** "Similar by": the signals on, the first counting most; empty: all of them, equally. */
export const similar = $state({ by: [] as Signal[] });

export function setSimilarBy(by: Signal[]) {
  similar.by = by;
  keep({ similar_by: by });
}

/** Radio's and Autoplay's defaults (the owner, 2026-10-25: Settings' Radio and Autoplay "should set defaults ... and
 *  also offer things like auto arm"; they "kick in when autoplay is initialized after first being off"): how each starts
 *  when switched on from off, and auto arm, whether a new queue starts with Autoplay on. Changed while playing, they
 *  are that queue's own. */
export type Defaults = {
  radio: { by: RadioBy; count: number };
  autoplay: { arm: boolean; mode: AutoplayMode; albums: boolean };
  skipFavourites: boolean;
};
export const defaults = $state<Defaults>({
  radio: { by: 'track', count: 5 },
  autoplay: { arm: true, mode: 'similar', albums: false },
  skipFavourites: false,
});
/** Whether the defaults were read (a server keeping none: the player keeps what it has). */
export const defaultsKnown = $state({ yes: false });
export function setDefaults(part: { radio?: Partial<Defaults['radio']>; autoplay?: Partial<Defaults['autoplay']>; skipFavourites?: boolean }) {
  if (part.radio) defaults.radio = { ...defaults.radio, ...part.radio };
  if (part.autoplay) defaults.autoplay = { ...defaults.autoplay, ...part.autoplay };
  if (part.skipFavourites !== undefined) defaults.skipFavourites = part.skipFavourites;
  const r = defaults.radio;
  const a = defaults.autoplay;
  keep({ radio: { by: r.by, count: r.count, skip_favourites: defaults.skipFavourites }, autoplay: { arm: a.arm, mode: a.mode, albums: a.albums } });
}

/** The Personal parts' own (Radio and Autoplay, Tracking), as kept. */
export const mine = $state({ moreFrom: { libraries: null, bandcamp: false } as MoreFrom, tracking: true, pickSkip: [] as string[], followable: false, autoAccept: [] as string[], known: false });
/** Others may follow this account or not (0.10, the Play On sheet's foot). */
export function setFollowable(on: boolean): Promise<void> {
  mine.followable = on;
  keep({ followable: on });
  return flush();
}
/** `user`'s tracks asked for go into the queue at once, or wait for an answer. */
export function setAutoAccept(user: string, on: boolean): Promise<void> {
  mine.autoAccept = on ? [...mine.autoAccept.filter((u) => u !== user), user] : mine.autoAccept.filter((u) => u !== user);
  keep({ auto_accept: mine.autoAccept });
  return flush();
}
/** The pick of the day moved on to the next that plays (Discover's menu of it): kept for the account, so every device
 *  shows the same; the last 50. */
export function skipPick(key: string) {
  mine.pickSkip = [...mine.pickSkip.filter((k) => k !== key), key].slice(-50);
  keep({ discover_pick_skip: mine.pickSkip });
}
/** What Radio and Autoplay choose from, changed: saved at once (the server reads it for every player of the account),
 *  so what they pick next comes from there. */
export function setMoreFrom(part: Partial<MoreFrom>): Promise<void> {
  mine.moreFrom = { ...mine.moreFrom, ...part };
  keep({ more_music: mine.moreFrom });
  return flush();
}
/** Whether a library of Cratarr's own is one Radio and Autoplay choose from. */
export const libraryOn = (id: string) => mine.moreFrom.libraries === null || mine.moreFrom.libraries.includes(id);
/** A library on or off, of `all` there are: every one on is all of them (null), as a library added later is then on
 *  too. */
export function turnLibrary(id: string, on: boolean, all: string[]) {
  const now = all.filter((x) => (x === id ? on : libraryOn(x)));
  return setMoreFrom({ libraries: now.length === all.length ? null : now });
}
export function setTracking(on: boolean) {
  mine.tracking = on;
  keep({ tracking: on });
}

let version = 0;
let settings: Record<string, unknown> = {};
let kept = true;
let changed: Partial<Choices> = {};
let timer: ReturnType<typeof setTimeout> | undefined;

/** Reads them; the choices among them, or null where none are kept (or the server keeps none). */
export async function loadMine(): Promise<Choices | null> {
  try {
    const r = await getMySettings();
    version = r.version;
    settings = r.settings ?? {};
    const by = (settings as Choices).similar_by;
    similar.by = Array.isArray(by) ? by.filter((x) => SIGNALS.some((s) => s.id === x)) : [];
    const from = (settings as Choices).more_music;
    mine.moreFrom = { libraries: Array.isArray(from?.libraries) ? from.libraries : null, bandcamp: !!from?.bandcamp };
    mine.tracking = (settings as Choices).tracking !== false;
    mine.followable = (settings as Choices).followable === true;
    const auto = (settings as Choices).auto_accept;
    mine.autoAccept = Array.isArray(auto) ? auto.filter((u) => typeof u === 'string') : [];
    mine.known = true;
    const skip = (settings as Choices).discover_pick_skip;
    mine.pickSkip = Array.isArray(skip) ? skip.filter((k) => typeof k === 'string') : [];
    const c = settings as Choices;
    if (c.radio) {
      defaults.radio = { by: c.radio.by ?? 'track', count: c.radio.count > 0 ? c.radio.count : 5 };
      defaults.skipFavourites = c.radio.skip_favourites === true;
    }
    if (c.autoplay === null) defaults.autoplay = { ...defaults.autoplay, arm: false };
    else if (c.autoplay) defaults.autoplay = { arm: c.autoplay.arm !== false, mode: c.autoplay.mode ?? 'similar', albums: c.autoplay.albums === true };
    defaultsKnown.yes = true;
    return r.version ? (settings as Choices) : null;
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) kept = false;
    return null;
  }
}

/** Keeps a change of them, a second after the last change. */
export function keep(part: Partial<Choices>) {
  if (!kept) return;
  changed = { ...changed, ...part };
  clearTimeout(timer);
  timer = setTimeout(save, 1000);
}

/** What waits to be kept, kept now. */
function flush(): Promise<void> {
  clearTimeout(timer);
  return kept ? save() : Promise.resolve();
}

async function save(again = true) {
  const part = changed;
  changed = {};
  try {
    version = (await putMySettings(version, { ...settings, ...part })).version;
    settings = { ...settings, ...part };
  } catch (e) {
    // Another device saved first: its settings, with this change over them, once more.
    if (again && e instanceof ApiError && e.status === 409) {
      const r = await getMySettings().catch(() => null);
      if (r) [version, settings] = [r.version, r.settings ?? {}];
      changed = { ...part, ...changed };
      return save(false);
    }
    if (e instanceof ApiError && e.status === 404) kept = false;
  }
}
