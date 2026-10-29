// What a filter row asks (lib/listFilter, lib/ConditionCard): kept apart from the filter's state, so its tests run in
// Node without the page.
import type { Condition } from './api';

/** An axis whose choice of none still asks something: in any playlist (the server's crates.rs). */
const NONE_ASKS: ReadonlySet<string> = new Set(['playlist']);

/** A condition that says what it asks. A choice of none (but a playlist's) or a range open at both ends asks nothing
 *  yet, so it filters nothing until a value is picked (the owner, 2026-10-29: a new Type row emptied the Library, "0
 *  of 2,972"; the server takes any of nothing as nothing). */
export function complete(c: Condition): boolean {
  const v = c.value;
  if (v.kind === 'any_of') return v.values.length > 0 || NONE_ASKS.has(c.axis);
  if (v.kind === 'range') return v.min !== null || v.max !== null;
  if (v.kind === 'like') return v.of.length > 0;
  return true;
}
