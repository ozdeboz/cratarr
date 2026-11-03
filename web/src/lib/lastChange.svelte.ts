// What the listener changed last of what can leave a list empty (decision 0095; the owner, 2026-11-02): where two
// causes would each bring things back on their own, the one changed last is what emptied the list in their eyes, and
// the popover points at it (lib/attribution pointAt). Said where each changes: the search's words, a crate's heart
// and disc, a filter's conditions and switches, the libraries switched off.
import type { ListReason } from './api';

export const lastChange = $state<{ kind: ListReason['kind'] | null; key?: string }>({ kind: null });

export function noteChange(kind: ListReason['kind'], key?: string) {
  lastChange.kind = kind;
  lastChange.key = key;
}

/** The cause changed last among `causes`, by its index; none known: null. */
export function lastOf(causes: readonly { kind: ListReason['kind']; key?: string }[]): number | null {
  if (!lastChange.kind) return null;
  const i = causes.findIndex((c) => c.kind === lastChange.kind && (lastChange.key === undefined || c.key === lastChange.key));
  return i < 0 ? null : i;
}
