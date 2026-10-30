// One commit (the owner, 2026-10-26, after the lab measured the panel and the pages coming in in pieces): what was
// shown stays, still, until what replaces it is ready, and then the whole of it shows at once ("All together"; "Old
// page stays, then switch"; Home "Hold until its first screen is ready"). lib/Hold.svelte holds a place (the panel, the
// page); what shows in it says it is not ready yet with holdUntil, and the place keeps a still picture of what it
// showed before (the shared loading sign over it after a moment) until it is. What says nothing shows at once.

import { getContext, untrack } from 'svelte';

export const HOLD = Symbol('hold');

export type HoldPlace = {
  /** Not ready: the place holds until the returned call (or HOLD_MAX_MS). */
  claim: () => () => void;
};

/** In a component's setup: while `ready()` is false for what `key()` names, the place it shows in holds what it showed
 *  before. A new key claims again (what it shows is no longer ready); ready ends the claim, as does leaving. */
export function holdUntil(key: () => unknown, ready: () => boolean) {
  const place = getContext<HoldPlace | undefined>(HOLD);
  if (!place) return;
  let done: (() => void) | null = null;
  const end = () => {
    const d = done;
    done = null;
    d?.();
  };
  // Before the place's DOM changes for the new key (lib/Hold takes its picture in its own pre-effect, first).
  $effect.pre(() => {
    void key();
    untrack(() => {
      end();
      done = place.claim();
    });
  });
  $effect(() => {
    if (ready()) untrack(end);
  });
  $effect(() => end);
}
