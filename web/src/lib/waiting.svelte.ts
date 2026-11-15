// One way of waiting (0.17; the owner, 2026-11-08: "we should have a unified way of showing progress"): every loader, a page's
// (lib/PageLoading) or a part's (lib/Asking), says under its mark in one line what it waits for, where that is more than an answer
// on its way: the library on this device opening for its first answer (lib/deviceAnswers), or a Cratarr that answers its health
// and not yet what was asked (lib/api's `watch`: busy, and waited for).

import { deviceOpening } from './deviceAnswers.svelte';

/** How many requests wait on a Cratarr that says it is there (lib/api `watched`). */
export const cratarrBusy = $state({ asks: 0 });

/** What a loader says it waits for, or null: an answer on its way, with nothing more to say. */
export function waitingFor(): string | null {
  if (deviceOpening.asks > 0) return 'Opening the library on this device';
  if (cratarrBusy.asks > 0) return 'Waiting for Cratarr';
  return null;
}
