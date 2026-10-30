// The kinds of record the Library shows (the owner, 2026-10-29: "albums should drop down to the big ones like EP,
// single, and other, possibly also compilation"; 0.10: Classical), picked beside back (lib/RecordsPicker) or from the
// sidebar's links under Library. One filter, shown both ways (the owner): a kind is the Library filter's Type row, which
// the Filter sheet shows and the funnel counts; a Type row made there shows as its kind, or none of them. Its rules are
// lib/recordKindRules'.
import { complete } from './filterRow';
import { filterOf, setConditions } from './listFilter.svelte';
import { library } from './albums.svelte';
import { can } from './capabilities.svelte';
import { kindOfRows, kindsOffered, rowsOfKind, type RecordKind } from './recordKindRules';

export { RECORD_KINDS, type RecordKind } from './recordKindRules';

const KIND = 'library';

/** Whether the server says which records are classical (0.10: its albums carry `classical`); a server before it does
 *  not, and Classical is not offered. */
export const classicalListed = () => can('classical') ?? !!library.view?.sources.some((s) => s.albums.some((a) => a.classical !== undefined));
/** The kinds offered now. */
export const recordKinds = () => kindsOffered(classicalListed());

/** The kind the filter's Type rows are; null: rows that are none of them. */
export function kindNow(): RecordKind | null {
  const on = filterOf(KIND).conditions.filter((c) => c.axis === 'type' && c.enabled !== false && complete(c));
  return kindOfRows(on, classicalListed());
}

/** The filter's Type rows replaced by the kind's one (none for Records, all of them). */
export function chooseKind(id: RecordKind) {
  const rest = filterOf(KIND).conditions.filter((c) => c.axis !== 'type');
  setConditions(KIND, [...rest, ...rowsOfKind(id, classicalListed())]);
}
