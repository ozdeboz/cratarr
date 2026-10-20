// A snapshot of the visuals as the switch keeps it (decision 0057): the patch whole, beside the settings' forms before
// it (shared.svelte.ts SNAPSHOT_V 1–3). One of those is converted when recalled, as it is (the owner: no polish of
// what was saved before).

import type { PatchV2 } from './ports';
import { DEFAULTS, keptFrom, type VisualSettings } from './settings.svelte';
import { toV2 } from './v2';
import { patchFrom } from './visuals-patch.svelte';
import './picture-wiring';

/** The snapshots' form with the patch. */
export const PATCH_SNAPSHOT_V = 4;

export const snapshotOfPatch = (p: PatchV2): Record<string, unknown> => ({ v: PATCH_SNAPSHOT_V, patch: structuredClone(p) });

/** A snapshot's patch: its own, or its settings converted. */
export function patchOfSnapshot(settings: Record<string, unknown>): PatchV2 {
  if (settings.v === PATCH_SNAPSHOT_V) {
    const p = patchFrom(settings.patch);
    if (p) return p;
  }
  return toV2(keptFrom({ ...structuredClone(DEFAULTS), ...settings }) as VisualSettings);
}
