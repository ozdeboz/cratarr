// The record kinds' rules (lib/recordKinds), pure so the tests take them as they are: which kinds the Library offers,
// the filter's Type row each one is, and which kind a Type row reads as. Classical (0.10; the owner, 2026-10-30) is
// offered where the server says which records are (its albums carry `classical`), after Compilations and before Other
// (the owner's order); Other is every record of none of the named kinds. A record shows in its own kind only where the
// server's precedence holds (Classical > Compilation > Album/EP/Single > Other; its filter switch, lib/pageView).
import type { FilterCondition } from './prefs.svelte';

export const BASE_NAMED = ['Album', 'EP', 'Single', 'Compilation'] as const;
export const CLASSICAL = 'Classical';

export const RECORD_KINDS = [
  { id: 'all', label: 'Records' },
  { id: 'Album', label: 'Albums' },
  { id: 'EP', label: 'EPs' },
  { id: 'Single', label: 'Singles' },
  { id: 'Compilation', label: 'Compilations' },
  { id: 'Classical', label: 'Classical' },
  { id: 'other', label: 'Other' },
] as const;
export type RecordKind = (typeof RECORD_KINDS)[number]['id'];

/** The kinds offered: Classical only where the server knows it. */
export const kindsOffered = (classical: boolean) => RECORD_KINDS.filter((k) => classical || k.id !== CLASSICAL);
/** The kinds named on their own (Other: none of them). */
export const namedKinds = (classical: boolean): string[] => (classical ? [...BASE_NAMED, CLASSICAL] : [...BASE_NAMED]);

const lower = (v: string) => v.toLowerCase();

/** The kind the Type rows are (`on`: those that apply); null: rows that are none of them. An Other row made before
 *  Classical was offered (without it among the named) still reads as Other. */
export function kindOfRows(on: readonly FilterCondition[], classical: boolean): RecordKind | null {
  if (!on.length) return 'all';
  const v = on.length === 1 && on[0].value.kind === 'any_of' ? on[0].value.values.map(lower) : null;
  if (!v) return null;
  if (!on[0].not && v.length === 1) return kindsOffered(classical).find((x) => x.id !== 'all' && x.id !== 'other' && lower(x.id) === v[0])?.id ?? null;
  const others = v.filter((t) => !BASE_NAMED.some((n) => lower(n) === t));
  if (on[0].not && BASE_NAMED.every((t) => v.includes(lower(t))) && (others.length === 0 || (others.length === 1 && others[0] === lower(CLASSICAL)))) return 'other';
  return null;
}

/** The Type rows a kind is: none for Records (all of them), Other as none of the named. */
export function rowsOfKind(id: RecordKind, classical: boolean): FilterCondition[] {
  if (id === 'all') return [];
  if (id === 'other') return [{ axis: 'type', not: true, value: { kind: 'any_of', values: namedKinds(classical) } }];
  return [{ axis: 'type', not: false, value: { kind: 'any_of', values: [id] } }];
}

/** A composer as Cratarr lists them (GET /api/classical/composers). */
export type Composer = { key: string; name: string; sort_name?: string | null; records: number; photo?: string | null };

/** By their sort name (Bach, Johann Sebastian), accents and case aside, as a library's index reads. */
export const byComposer = (a: Composer, b: Composer) =>
  (a.sort_name || a.name).localeCompare(b.sort_name || b.name, undefined, { sensitivity: 'base' }) || a.name.localeCompare(b.name);
