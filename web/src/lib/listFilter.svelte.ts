// A list's filter (decisions 0060 and 0061): its conditions, a crate's unsaved, and its kind's own switches
// (lib/pageView SWITCHES), remembered per kind of list (a page's id, or a panel kind) in the prefs. The Filter sheet
// changes them (lib/FilterSheet); the lists read them.
import { filterList, type Condition, type FilterList } from './api';
import { SWITCHES } from './pageView.svelte';
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
// Only the switches the list still offers: one left on from before (Lossless only, say) is no filter now.
export const filtersOn = (kind: string) =>
  activeConditions(kind).length + filterOf(kind).on.filter((k) => (SWITCHES[kind] ?? []).some((w) => w.key === k)).length;

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
 *  null while there are none, or the server cannot tell (then nothing is left out). `pending`: conditions apply and
 *  their answer is on its way; the last answer for the same list stays meanwhile, and before the first a list shows
 *  as loading rather than unfiltered (the owner, 2026-10-23: nothing shown that something else then replaces). */
export function passing(kindOf: () => string, get: () => { list: FilterList; source?: string } | null): { keys: Set<string> | null; pending: boolean } {
  const out = $state<{ keys: Set<string> | null; pending: boolean }>({ keys: null, pending: false });
  let of = '';
  $effect(() => {
    const kind = kindOf();
    const at = get();
    inView[kind] = at ?? undefined;
    const conditions = conditionsFor(kind);
    const without = at?.list.kind === 'library' ? hiddenLibraries() : undefined;
    // A crate's rows come filtered (CrateLook.filter): its answer has no keys.
    if (!at || !conditions || at.list.kind === 'crate') {
      out.keys = null;
      out.pending = false;
      of = '';
      return;
    }
    // Another list: the last one's answer says nothing about it.
    const list = JSON.stringify([kind, at]);
    if (list !== of) {
      of = list;
      out.keys = null;
    }
    out.pending = true;
    let gone = false;
    const t = setTimeout(() => {
      filterList(at.list, conditions, at.source, without).then(
        (f) => {
          if (gone) return;
          out.keys = new Set(f.keys);
          out.pending = false;
        },
        () => {
          if (gone) return;
          out.keys = null;
          out.pending = false;
        },
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  return out;
}
