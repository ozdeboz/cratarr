// The snapshot library (0.9.18; the owner, 2026-10-29: "why can I not find them in the snapshots panel? This is where
// snapshots should be"): one list, read by the Snapshots sheet and by the panel's Presets alike. The saved snapshots
// first (the server's, every device's), then the example racks (examples.ts), each group under its heading. Pure: the
// saved ones are given, so a test reads it as the pages do.

import { EXAMPLES } from './examples';

export type LibraryGroup = 'saved' | 'examples';
/** Each group's heading, in the library's order. */
export const LIBRARY_GROUPS: readonly { id: LibraryGroup; label: string }[] = [
  { id: 'saved', label: 'Saved' },
  { id: 'examples', label: 'Examples' },
];

/** A snapshot in the library: its id, its name (a saved one's without the time it was saved at), its line (an
 *  example's sentence; none for a saved one), its group, and the saved snapshot it is, if it is one. */
export type LibraryEntry<S> = { id: string; label: string; line: string; group: LibraryGroup; saved?: S };

/** The saved ones' names end in the time they were saved (" · 21:04"). */
const SAVED_AT = / · \d\d:\d\d$/;

/** The library: the saved snapshots as given, then the examples. */
export function libraryOf<S extends { id: string; name: string }>(saved: readonly S[]): LibraryEntry<S>[] {
  return [
    ...saved.map((s) => ({ id: s.id, label: s.name.replace(SAVED_AT, ''), line: '', group: 'saved' as const, saved: s })),
    ...EXAMPLES.map((x) => ({ id: x.id, label: x.label, line: x.line, group: 'examples' as const })),
  ];
}

/** The library by its groups, each with its heading, the empty ones left out. */
export function libraryGroups<S>(entries: readonly LibraryEntry<S>[]): { id: LibraryGroup; label: string; entries: LibraryEntry<S>[] }[] {
  return LIBRARY_GROUPS.map((g) => ({ ...g, entries: entries.filter((e) => e.group === g.id) })).filter((g) => g.entries.length);
}
