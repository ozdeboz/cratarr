// One's own settings (decision 0051, several users), kept by the server for each account so they follow its user from
// device to device: the More music sheet's choices, Radio's (by what, how many, leaving favourites out) and Autoplay's
// (its mode and albums, or off). A page that starts with nothing playing takes them (localPlayer `startWith`); every
// change of them is kept (`keep`), a second after the last. A server from before several users keeps none: nothing is
// sent to it again.
import { ApiError, getMySettings, putMySettings } from './api';
import type { AutoplayMode, RadioBy } from './localPlayer.svelte';

export type Choices = {
  radio?: { by: RadioBy; count: number; skip_favourites: boolean };
  autoplay?: { mode: AutoplayMode; albums: boolean } | null;
};

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
