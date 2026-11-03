// The Library's release types (0.13; the owner, 2026-10-31: three layers), pure so the tests take them as they are.
// A record has one release type (Album, EP, Single, Broadcast, Other), any number of extra ones (Compilation, Live,
// Soundtrack…) and may be Classical; it counts under every type it has, and the server reads the filter's Type rows
// "or within a layer, and across" ([Album, Live]: live albums). The picker beside back lists the Type axis as the
// server gives it (every value with records, most records first), so no list of types is kept here to fall out of
// step with it; its label reads the Type rows only.
import type { FilterCondition } from './prefs.svelte';

export const ALL = 'all';
export const CLASSICAL = 'Classical';
/** The label of Type rows no one type names (two values, Not, two rows). */
export const SELECTED = 'Selected types';

/** A type as the picker names what shows: in the plural ("Albums", "EPs"), or as it is where that reads better ("Live",
 *  "Classical", "Spoken word"). */
const PLURAL: Record<string, string> = {
  album: 'Albums',
  ep: 'EPs',
  single: 'Singles',
  broadcast: 'Broadcasts',
  compilation: 'Compilations',
  soundtrack: 'Soundtracks',
  remix: 'Remixes',
  'dj-mix': 'DJ-mixes',
  'mixtape/street': 'Mixtapes',
  demo: 'Demos',
  spokenword: 'Spoken word',
  interview: 'Interviews',
  audiobook: 'Audiobooks',
  'audio drama': 'Audio dramas',
  'field recording': 'Field recordings',
};
export const typeLabel = (value: string) => PLURAL[value.toLowerCase()] ?? value;

/** A record as its release type names it (the owner, 2026-11-01: "When it's a single type, we can use that type"):
 *  album, EP, single, broadcast; a record where it has none or is Other. */
const ONE: Record<string, string> = { album: 'album', ep: 'EP', single: 'single', broadcast: 'broadcast' };
export const recordWord = (type?: string | null) => ONE[type?.toLowerCase() ?? ''] ?? 'record';
/** Several records' word: their one release type where they all have it ("3 EPs"), else record ("12 records"). */
export function recordsWord(types: readonly (string | null | undefined)[]): string {
  const words = new Set(types.map(recordWord));
  return words.size === 1 ? [...words][0] : 'record';
}
/** As a heading: Albums, EPs, Singles, Broadcasts, Records. */
export const recordsTitle = (types: readonly (string | null | undefined)[]) => {
  const w = recordsWord(types);
  return `${w[0].toUpperCase()}${w.slice(1)}s`;
};

export type TypeOption = { id: string; label: string; count?: number };
/** The Type axis as the filter answers it (lib/api Axis): its values, most records first, and their counts. */
export type TypeAxis = { values: readonly string[]; counts?: readonly number[] };

/** What the picker offers: Records, then every type with records, in the axis's order. */
export function typeOptions(axis: TypeAxis | null): TypeOption[] {
  const types = (axis?.values ?? []).map((v, i) => ({ id: v, label: typeLabel(v), count: axis?.counts?.[i] })).filter((o) => o.count === undefined || o.count > 0);
  return [{ id: ALL, label: 'Records' }, ...types];
}

/** The type the Type rows (`on`: those that apply) are: Records for none, the one value of one plain "any of" row;
 *  null for anything else, which reads as Selected types and ticks nothing. */
export function typeOfRows(on: readonly FilterCondition[]): string | null {
  if (!on.length) return ALL;
  const [row] = on;
  if (on.length === 1 && !row.not && row.value.kind === 'any_of' && row.value.values.length === 1) return row.value.values[0];
  return null;
}

/** The types the Type rows select, one tick each in the picker (the owner, 2026-11-02: "it could very well show one
 *  check per selected type"): Records for none; the values of every row that takes them in (not those a Not row
 *  leaves out). */
export function typesTicked(on: readonly FilterCondition[]): string[] {
  if (!on.length) return [ALL];
  return [...new Set(on.flatMap((c) => (!c.not && (c.value.kind === 'any_of' || c.value.kind === 'all_of') ? c.value.values : [])))];
}

/** The Type rows with `id` taken in or out (a long-press in the picker, the owner the same day: "a quick way to modify
 *  the selection"): one row of what is ticked, the server reading it as the Filter sheet's card says it ("or" within a
 *  layer, "and" across); none left, Records. Records itself: none. */
export function toggledRows(on: readonly FilterCondition[], id: string): FilterCondition[] {
  if (id === ALL) return [];
  const now = typesTicked(on).filter((v) => v !== ALL);
  const next = now.includes(id) ? now.filter((v) => v !== id) : [...now, id];
  return next.length ? [{ axis: 'type', not: false, value: { kind: 'any_of', values: next } }] : [];
}

/** The picker's label for the Type rows. */
export function typesShown(on: readonly FilterCondition[]): string {
  const t = typeOfRows(on);
  return t === null ? SELECTED : t === ALL ? 'Records' : typeLabel(t);
}

/** A Type row's values as the Filter sheet's card says them (the owner, 2026-10-31, mockup A), as the server reads
 *  them: "or" within a layer, "and" across, the layers in order ("Album or EP and Live and Classical"); a Not row means
 *  the same, left out. `layerOf`: a value's layer (the axis's `layers`); one it does not know is its own. */
export function typesSaid(values: readonly string[], layerOf: (v: string) => number | undefined, shown: (v: string) => string = (v) => v): string {
  const groups = new Map<number | string, string[]>();
  for (const v of values) {
    const l = layerOf(v) ?? `own:${v}`;
    groups.set(l, [...(groups.get(l) ?? []), shown(v)]);
  }
  const order = (k: number | string) => (typeof k === 'number' ? k : 99);
  return [...groups.entries()].sort(([a], [b]) => order(a) - order(b)).map(([, vs]) => vs.join(' or ')).join(' and ');
}

/** A value's layer, as the Type axis says it. */
export const layerIn = (axis: { values: readonly string[]; layers?: readonly number[] } | null | undefined) => (v: string) => {
  const i = axis?.values.indexOf(v) ?? -1;
  return i >= 0 ? axis?.layers?.[i] : undefined;
};

/** The Type rows a pick is: none for Records, else that type's one. */
export const rowsOfType = (id: string): FilterCondition[] => (id === ALL ? [] : [{ axis: 'type', not: false, value: { kind: 'any_of', values: [id] } }]);

const NAMED_BEFORE = ['album', 'ep', 'single', 'compilation'];
/** Type rows saved before the layers, read once as the layers have them: Incomplete is no type now (a record missing
 *  tracks is its full release's type), so it leaves the rows (a row of it alone goes); the old Other, "not any of" the
 *  kinds named then, becomes "any of Other". */
export function layeredRows(conditions: readonly FilterCondition[]): FilterCondition[] {
  return conditions.flatMap((c): FilterCondition[] => {
    if (c.axis !== 'type' || (c.value.kind !== 'any_of' && c.value.kind !== 'all_of')) return [c];
    const values = c.value.values;
    if (c.not && NAMED_BEFORE.every((n) => values.some((v) => v.toLowerCase() === n))) return [{ ...c, not: false, value: { kind: 'any_of', values: ['Other'] } }];
    const kept = values.filter((v) => v.toLowerCase() !== 'incomplete');
    if (kept.length === values.length) return [c];
    return kept.length ? [{ ...c, value: { ...c.value, values: kept } }] : [];
  });
}
