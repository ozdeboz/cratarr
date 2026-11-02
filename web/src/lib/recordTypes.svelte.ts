// The Library's release types (0.13; the owner, 2026-10-31), picked beside back (lib/RecordsPicker) or from the
// sidebar's links under Library. One filter, shown both ways: a pick is the Library filter's Type rows, which the Filter
// sheet shows and the funnel counts. What is offered is the Type axis as the server counts it over the Library (lib/api
// filterList, the libraries switched on), so the page keeps no list of types of its own. Its rules are
// lib/recordTypeRules'.
import { filterList } from './api';
import { library } from './albums.svelte';
import { can } from './capabilities.svelte';
import { localMode } from './deviceMusic.svelte';
import { complete } from './filterRow';
import { askAtOnce, filterOf, filterStep, hiddenLibraries, setConditions } from './listFilter.svelte';
import { prefs } from './prefs.svelte';
import { ALL, rowsOfType, typeOfRows, typeOptions, typesShown, type TypeAxis, type TypeOption } from './recordTypeRules';

const KIND = 'library';

/** Whether the server says which records are classical (0.10: its albums carry `classical`). */
export const classicalListed = () => can('classical') ?? !!library.view?.sources.some((s) => s.albums.some((a) => a.classical !== undefined));

const types = $state<{ axis: TypeAxis | null }>({ axis: null });
let askedFor: unknown[] = [];
/** The Type axis, asked for again when the Library, its server or the libraries switched on change. Read in an effect
 *  (App: the picker and the sidebar's links show it). */
export function loadTypes() {
  if (localMode()) return;
  const now = [library.view, prefs.source, prefs.hiddenLibraries.join('\n')];
  if (now.every((x, i) => x === askedFor[i])) return;
  askedFor = now;
  filterList({ kind: KIND }, [], prefs.source || undefined, hiddenLibraries()).then(
    (f) => {
      if (askedFor === now) types.axis = f.axes.find((a) => a.key === 'type') ?? null;
    },
    () => {
      /* the picker offers Records and what is chosen; the Library says what went wrong */
    },
  );
}

/** The Library filter's Type rows that apply. */
const typeRows = () => filterOf(KIND).conditions.filter((c) => c.axis === 'type' && c.enabled !== false && complete(c));

/** The one type the rows are (Records: none); null where they are more (Selected types). */
export const typeNow = () => typeOfRows(typeRows());
/** What the picker reads. */
export const typesLabel = () => typesShown(typeRows());

/** What the picker offers: Records and every type with records, the one chosen among them while the axis comes. */
export function typesOffered(): TypeOption[] {
  const options = typeOptions(types.axis);
  const t = typeNow();
  return t && !options.some((o) => o.id === t) ? [...options, { id: t, label: typesShown(typeRows()) }] : options;
}

/** The filter's Type rows replaced by the pick's (none for Records): one step of the history (`step`: on the Library;
 *  a sidebar link elsewhere goes there instead). */
export function chooseType(id: string, step = true) {
  const set = () => {
    const was = JSON.stringify(filterOf(KIND).conditions);
    setConditions(KIND, [...filterOf(KIND).conditions.filter((c) => c.axis !== 'type'), ...rowsOfType(id)]);
    if (JSON.stringify(filterOf(KIND).conditions) !== was) askAtOnce(KIND);
  };
  if (step) filterStep(KIND, set);
  else set();
}

export { ALL };
