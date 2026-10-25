// One's own settings (decision 0051, several users), kept by the server for each account so they follow its user from
// device to device: the More music sheet's choices, Radio's (by what, how many, leaving favourites out) and Autoplay's
// (its mode and albums, or off), and what makes music alike for both ("Similar by", which the server reads itself for
// each choice). A page that starts with nothing playing takes them (localPlayer `startWith`); every
// change of them is kept (`keep`), a second after the last. A server from before several users keeps none: nothing is
// sent to it again.
import { ApiError, getMySettings, putMySettings } from './api';
import type { AutoplayMode, RadioBy } from './localPlayer.svelte';

export type Choices = {
  radio?: { by: RadioBy; count: number; skip_favourites: boolean };
  autoplay?: { mode: AutoplayMode; albums: boolean } | null;
  similar_by?: Signal[];
  /** What Radio and Autoplay choose from (the owner, 2026-10-25): Cratarr's own libraries (null: all of them), and
   *  Discover's suggestions on Bandcamp. */
  more_music?: MoreFrom;
  /** Whether the account's plays are kept at all (the owner, 2026-10-25; absent: on). */
  tracking?: boolean;
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

/** The Personal parts' own (Radio and Autoplay, Tracking), as kept. */
export const mine = $state({ moreFrom: { libraries: null, bandcamp: false } as MoreFrom, tracking: true });
export function setMoreFrom(part: Partial<MoreFrom>) {
  mine.moreFrom = { ...mine.moreFrom, ...part };
  keep({ more_music: mine.moreFrom });
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
