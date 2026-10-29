// The visuals' patch (decision 0057, the switch, S1): the one truth of what is drawn and how it moves, a v2 patch
// (modules, cables, macros, pins; devices unopened). The panel and the graph change it here, the frame loop reads it.
// Kept on this device under its own key; where there is none yet, made once from the settings kept before the switch
// (toV2), which stay as they were. This device's own settings (Quality, FPS, Brightness) are no part of it: they stay
// in settings.svelte.ts. Undo and redo are patch-store.ts's, a run of changes to one thing (a knob dragged) one step.

import { canRedo, canUndo, change as changed, newStore, redo as redone, replace as replaced, undo as undone, type Store } from './patch-store';
import type { PatchV2 } from './ports';
import { visualSettings, type VisualSettings } from './settings.svelte';
import { toV2 } from './v2';
import { upgraded } from './upgrade';
import { v2TypeRecord } from './v2-types';
// The picture side of the conversion (toV2's layers, loops and finish as cables).
import './picture-wiring';

export const PATCH_KEY = 'cratarr-visuals-patch';

/** An empty rack: only what every rack has (0057: the Meter and the Screen, its flash guard never removed); Music's
 *  signals come as modules once a cable wants one. */
export const emptyRack = (): PatchV2 => ({ v: 2, modules: ['meter', 'screen'].map((type) => ({ id: type, type, knobs: {} })), cables: [], macros: [], pins: [] });
/** How long after the last change it is kept (ms): a knob dragged changes it every frame. */
const SAVE_MS = 500;

/** A kept patch, if it is one. */
export function patchFrom(kept: unknown): PatchV2 | null {
  const p = kept as Partial<PatchV2> | null;
  return p && p.v === 2 && Array.isArray(p.modules) && Array.isArray(p.cables) ? { macros: [], pins: [], ...p } as PatchV2 : null;
}

function load(): PatchV2 {
  try {
    const kept = patchFrom(JSON.parse(localStorage.getItem(PATCH_KEY) ?? 'null'));
    // Kept from before: made current as it comes (upgrade.ts).
    if (kept) return upgraded(kept, v2TypeRecord());
  } catch {
    // None kept, or none readable: made from the settings.
  }
  return toV2($state.snapshot(visualSettings) as VisualSettings);
}

let store = $state.raw<Store>(newStore(load()));
let saving: ReturnType<typeof setTimeout> | undefined;

function keep() {
  clearTimeout(saving);
  saving = setTimeout(() => {
    try {
      localStorage.setItem(PATCH_KEY, JSON.stringify(store.now));
    } catch {
      // Full or blocked: it stays as it is for this visit.
    }
  }, SAVE_MS);
}

function set(next: Store) {
  if (next === store) return;
  store = next;
  keep();
}

export const visualsPatch = {
  /** The patch now (unflattened: devices as devices). A new object on every change; never changed in place. */
  get patch(): PatchV2 {
    return store.now;
  },
  get canUndo() {
    return canUndo(store);
  },
  get canRedo() {
    return canRedo(store);
  },
  /** A change, made by `f` on a copy (it may change it in place and return nothing). `key` (`knob:<module>.<knob>`,
   *  `cable:<id>` …): changes to the same thing close together are one undo step. */
  change(f: (p: PatchV2) => PatchV2 | void, key: string | null = null) {
    set(changed(store, f, key));
  },
  /** A change the visuals make as they play (a moment, a new track's look): a performance, not an edit, so no undo
   *  step; an undo after it goes back to the edit before. */
  perform(f: (p: PatchV2) => PatchV2 | void) {
    const draft = structuredClone(store.now);
    const next = f(draft) ?? draft;
    set({ ...store, now: next });
  },
  /** A patch from elsewhere (a preset, a snapshot, the phone driving this device): the patch now, one undo step. */
  replace(p: PatchV2) {
    set(replaced(store, upgraded($state.snapshot(p) as PatchV2, v2TypeRecord())));
  },
  undo() {
    set(undone(store));
  },
  redo() {
    set(redone(store));
  },
};
