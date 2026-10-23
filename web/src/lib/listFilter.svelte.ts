// A list's filter (decisions 0060 and 0061): its conditions, a crate's unsaved, and its kind's own switches
// (lib/pageView SWITCHES), remembered per kind of list (a page's id, or a panel kind) in the prefs. The Filter sheet
// changes them (lib/FilterSheet); the lists read them.
import { filterList, type Condition, type FilterList } from './api';
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

/** The libraries switched off in the filter (`source/library`), for the server's crates (decision 0024, amended
 *  2026-10-23: a crate fills from every library switched on); none: undefined. */
export const hiddenLibraries = () => (prefs.hiddenLibraries.length ? [...prefs.hiddenLibraries] : undefined);

/** The conditions that apply, as the server takes them; none: undefined. */
export function conditionsFor(kind: string): Condition[] | undefined {
  const on = activeConditions(kind);
  return on.length ? on : undefined;
}

/** What each kind of list in view is to the server (lib/api FilterList), so the Filter sheet can ask for its axes with
 *  their counts; set by the list while it shows. */
export const inView = $state<Record<string, { list: FilterList; source?: string } | undefined>>({});

/** The keys of a list's items (of the kind `kindOf` says) its conditions let through (lib/api filterList), asked again 300 ms after they change;
 *  null while there are none, or the server cannot tell (then nothing is left out). */
export function passing(kindOf: () => string, get: () => { list: FilterList; source?: string } | null): { keys: Set<string> | null } {
  const out = $state<{ keys: Set<string> | null }>({ keys: null });
  $effect(() => {
    const kind = kindOf();
    const at = get();
    inView[kind] = at ?? undefined;
    const conditions = conditionsFor(kind);
    const without = at?.list.kind === 'library' ? hiddenLibraries() : undefined;
    // A crate's rows come filtered (CrateLook.filter): its answer has no keys.
    if (!at || !conditions || at.list.kind === 'crate') {
      out.keys = null;
      return;
    }
    let gone = false;
    const t = setTimeout(() => {
      filterList(at.list, conditions, at.source, without).then(
        (f) => !gone && (out.keys = new Set(f.keys)),
        () => !gone && (out.keys = null),
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  return out;
}
