// The patch's store (decision 0057 §7): the one place a v2 patch is changed, by the graph and the rack alike, with undo
// and redo (VCV Rack's history). Pure: each change is a function from the patch to the next, kept as the patch before
// it. A run of changes with the same `key` within `COALESCE_MS` (a knob being dragged, a curve being drawn) is one
// step. The Svelte side holds a Store in its state and calls these.

import type { PatchV2 } from './ports';

/** How many steps back undo goes. */
export const DEPTH = 100;
/** Changes with the same key this close together are one step (ms). */
export const COALESCE_MS = 800;

export type Store = {
  now: PatchV2;
  /** The patches before, the latest last; and the ones undone, the latest undone last. */
  past: PatchV2[];
  future: PatchV2[];
  /** The last change's key and when it was (to coalesce a drag). */
  last: { key: string | null; at: number };
};

export const newStore = (p: PatchV2): Store => ({ now: p, past: [], future: [], last: { key: null, at: -Infinity } });

const copy = (p: PatchV2): PatchV2 => structuredClone(p);

/** A change: `f` makes the next patch from a copy of this one (it may change the copy in place and return it). A change
 *  that leaves the patch as it was is no step. `key`: what it changes (`knob:<module>.<knob>`), so a drag is one step. */
export function change(s: Store, f: (p: PatchV2) => PatchV2 | void, key: string | null = null, at = Date.now()): Store {
  const draft = copy(s.now);
  const next = f(draft) ?? draft;
  if (JSON.stringify(next) === JSON.stringify(s.now)) return s;
  const same = key !== null && key === s.last.key && at - s.last.at < COALESCE_MS;
  const past = same ? s.past : [...s.past, s.now].slice(-DEPTH);
  return { now: next, past, future: [], last: { key, at } };
}

export const canUndo = (s: Store) => s.past.length > 0;
export const canRedo = (s: Store) => s.future.length > 0;

export function undo(s: Store): Store {
  if (!canUndo(s)) return s;
  return { now: s.past[s.past.length - 1], past: s.past.slice(0, -1), future: [...s.future, s.now], last: { key: null, at: -Infinity } };
}

export function redo(s: Store): Store {
  if (!canRedo(s)) return s;
  return { now: s.future[s.future.length - 1], past: [...s.past, s.now], future: s.future.slice(0, -1), last: { key: null, at: -Infinity } };
}

/** A patch from elsewhere (the group, a recall): the patch now, undoable as one step. */
export const replace = (s: Store, p: PatchV2, at = Date.now()): Store => change(s, () => copy(p), null, at);
