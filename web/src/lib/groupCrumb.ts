// A panel's group as a level of its breadcrumb (the owner, 2026-11-02: the picker of a part's siblings was "way too
// tall"; the sections as the middle layer). Settings' groups, Health's areas and Insights' groups: a picker of the
// groups, a group's pick landing on the part last seen in it, or its top part where none is (or that one is gone); the
// part's own picker lists only its group's parts. Not a place: no page of its own, and Back walks past it. Pure, for
// the tests.

/** A part, as its panel lists it: its id and its group, in the panel's order. */
export type Grouped = { id: string; group: string };

/** The groups in their order, each once. */
export function groupsOf(parts: readonly Grouped[]): string[] {
  return [...new Set(parts.map((p) => p.group))];
}

/** Where a group's pick lands: the part last seen in it while it is still one of the group's, else its top one; null
 *  for a group with none. */
export function landingOf(parts: readonly Grouped[], group: string, last?: string | null): string | null {
  const own = parts.filter((p) => p.group === group);
  return own.find((p) => p.id === last)?.id ?? own[0]?.id ?? null;
}

/** The parts last seen, a part's group now `id`'s: `panel/group` → part (prefs.groupLast). The same object where
 *  nothing changes, so nothing is written again. */
export function remembered(last: Readonly<Record<string, string>>, panel: string, group: string, id: string): Record<string, string> {
  const key = `${panel}/${group}`;
  return last[key] === id ? last : { ...last, [key]: id };
}
