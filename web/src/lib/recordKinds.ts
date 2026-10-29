// The kinds of record the Library shows (the owner, 2026-10-29: "albums should drop down to the big ones like EP,
// single, and other, possibly also compilation"), picked beside back (lib/RecordsPicker) or from the sidebar's links
// under Library. One filter, shown both ways (the owner): a kind is the Library filter's Type row, which the Filter
// sheet shows and the funnel counts; a Type row made there shows as its kind, or none of them.
import type { FilterCondition } from './prefs.svelte';
import { complete } from './filterRow';
import { filterOf, setConditions } from './listFilter.svelte';

const KIND = 'library';
/** The kinds named on their own; Other is every other (a live album's, a soundtrack's) and the records of none. A
 *  record has its kinds primary first ("Album", "Compilation"), so an album that is a compilation is both. */
const NAMED = ['Album', 'EP', 'Single', 'Compilation'];
export const RECORD_KINDS = [
  { id: 'all', label: 'Records' },
  { id: 'Album', label: 'Albums' },
  { id: 'EP', label: 'EPs' },
  { id: 'Single', label: 'Singles' },
  { id: 'Compilation', label: 'Compilations' },
  { id: 'other', label: 'Other' },
] as const;
export type RecordKind = (typeof RECORD_KINDS)[number]['id'];
const lower = (v: string) => v.toLowerCase();

/** The kind the filter's Type rows are; null: rows that are none of them. */
export function kindNow(): RecordKind | null {
  const on = filterOf(KIND).conditions.filter((c) => c.axis === 'type' && c.enabled !== false && complete(c));
  if (!on.length) return 'all';
  const v = on.length === 1 && on[0].value.kind === 'any_of' ? on[0].value.values.map(lower) : null;
  if (!v) return null;
  if (!on[0].not && v.length === 1) return RECORD_KINDS.find((x) => lower(x.id) === v[0])?.id ?? null;
  if (on[0].not && v.length === NAMED.length && NAMED.every((t) => v.includes(lower(t)))) return 'other';
  return null;
}

/** The filter's Type rows replaced by the kind's one (none for Records, all of them). */
export function chooseKind(id: RecordKind) {
  const rest = filterOf(KIND).conditions.filter((c) => c.axis !== 'type');
  const row: FilterCondition[] =
    id === 'all' ? [] : id === 'other' ? [{ axis: 'type', not: true, value: { kind: 'any_of', values: [...NAMED] } }] : [{ axis: 'type', not: false, value: { kind: 'any_of', values: [id] } }];
  setConditions(KIND, [...rest, ...row]);
}
