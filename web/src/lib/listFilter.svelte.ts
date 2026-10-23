// A list's filter (decisions 0060 and 0061): its conditions, a crate's unsaved, and its kind's own switches
// (lib/pageView SWITCHES), remembered per kind of list (a page's id, or a panel kind) in the prefs. The Filter sheet
// changes them (lib/FilterSheet); the lists read them.
import { prefs, type FilterCondition, type ListFilter } from './prefs.svelte';

const NO_FILTER: ListFilter = { conditions: [], on: [] };

/** A kind of list's filter as remembered (none yet: empty). */
export const filterOf = (kind: string): ListFilter => prefs.filters[kind] ?? NO_FILTER;

/** One of a kind of list's own switches is on. */
export const switchOn = (kind: string, key: string) => filterOf(kind).on.includes(key);

export function setSwitch(kind: string, key: string, on: boolean) {
  const f = filterOf(kind);
  const rest = f.on.filter((k) => k !== key);
  prefs.filters[kind] = { ...f, on: on ? [...rest, key] : rest };
}

export function setConditions(kind: string, conditions: FilterCondition[]) {
  prefs.filters[kind] = { ...filterOf(kind), conditions };
}

/** A kind of list's conditions that apply (not muted). */
export const activeConditions = (kind: string) => filterOf(kind).conditions.filter((c) => c.enabled !== false);

/** How many filters a kind of list has on (the funnel's count): its conditions that apply and its switches on. */
export const filtersOn = (kind: string) => activeConditions(kind).length + filterOf(kind).on.length;

/** Everything off again (the sheet's Clear). */
export function clearFilter(kind: string) {
  delete prefs.filters[kind];
}
