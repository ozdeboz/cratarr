// A list's filter (decisions 0060 and 0061): its conditions, a crate's unsaved, and its kind's own switches
// (lib/pageView SWITCHES), remembered per kind of list (a page's id, or a panel kind) in the prefs. The Filter sheet
// changes them (lib/FilterSheet); the lists read them.
import { filterList, type Condition, type FilterList } from './api';
import { complete } from './filterRow';
import { SWITCHES } from './pageView.svelte';
import { prefs, type FilterCondition, type ListFilter } from './prefs.svelte';
import { toast } from './toast.svelte';

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

/** Back to no filter of the list's own: no conditions, every switch off (the Filter sheet's Reset, the owner,
 *  2026-10-29). The libraries switched off stay so: they are the one filter that holds everywhere. */
export function resetFilter(kind: string) {
  prefs.filters[kind] = { conditions: [], on: [] };
}

/** A kind of list's conditions that apply: not muted, and saying what they ask (lib/filterRow). */
export const activeConditions = (kind: string) => filterOf(kind).conditions.filter((c) => c.enabled !== false && complete(c));

/** A kind of list's own switches that are on, of those it still offers: one left on from before (Lossless only, say)
 *  is no filter now. */
export const switchesOn = (kind: string) => filterOf(kind).on.filter((k) => (SWITCHES[kind] ?? []).some((w) => w.key === k));

/** How many filters a kind of list has on (the funnel's count): its conditions that apply and its switches on. */
export const filtersOn = (kind: string) => activeConditions(kind).length + switchesOn(kind).filter((k) => !(SWITCHES[kind] ?? []).find((w) => w.key === k)?.quiet).length;

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

/** The keys of a list's items (of the kind `kindOf` says) its filter lets through: its conditions and its own switches,
 *  one question to the server, which holds what each switch means (lib/api filterList; the owner, 2026-10-29: one
 *  filter, one engine, so no list filters a switch itself), asked again 300 ms after they change; null while none apply
 *  (a failed ask keeps the last answer, and says so). `pending`: they apply and their answer is on its way; the last
 *  answer for the same list stays meanwhile, and before the first a list shows as loading rather than unfiltered (the
 *  owner, 2026-10-23: nothing shown that something else then replaces). */
export function passing(kindOf: () => string, get: () => { list: FilterList; source?: string } | null): { keys: Set<string> | null; pending: boolean } {
  const out = $state<{ keys: Set<string> | null; pending: boolean }>({ keys: null, pending: false });
  let of = '';
  $effect(() => {
    const kind = kindOf();
    const at = get();
    inView[kind] = at ?? undefined;
    // A plain copy to send, read whole: a row's every field asks again (a card hands back a new row, lib/ConditionCard;
    // the owner, 2026-10-29: Not, then a chip, never asked the server again while rows were changed in place).
    const conditions = $state.snapshot(conditionsFor(kind)) ?? [];
    const switches = switchesOn(kind);
    const without = at && (at.list.kind === 'library' || at.list.kind === 'artists') ? hiddenLibraries() : undefined;
    // A crate's rows come filtered (CrateLook.filter): its answer has no keys.
    if (!at || (!conditions.length && !switches.length) || at.list.kind === 'crate') {
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
      filterList(at.list, conditions, at.source, without, switches).then(
        (f) => {
          if (gone) return;
          out.keys = new Set(f.keys);
          out.pending = false;
        },
        (e) => {
          if (gone) return;
          // Said, the last answer kept: it showed the whole list with the funnel still lit, as if filtered (the
          // architecture audit, 2026-10-29).
          out.pending = false;
          toast.fail(e, "The filter couldn't be applied");
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
