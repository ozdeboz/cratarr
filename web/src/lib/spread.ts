// A record's tracks kept apart in a play order (0.12.1; the owner, 2026-10-31: a grabbed crate's "big play button had
// too much repetition of records in the queue"): the rules crate's spread, as the server's crate Play and Shuffle have
// it (0.13, "One spread": in the matcher's module, lib/matcherLoad's `matcherModule`). The order given stays the base;
// a record comes again only `gap` other records later where the tracks allow (`gap` at most one less than how many
// records there are).
import type { Matcher } from './matcher';

/** `items` in their order with each record's tracks at least `gap` other records apart where they can be. */
export function spreadRecords<T>(module: Matcher, items: readonly T[], recordOf: (t: T) => string, gap = 4): T[] {
  const numbers = new Map<string, number>();
  const keys = items.map((t) => {
    const r = recordOf(t);
    let k = numbers.get(r);
    if (k === undefined) numbers.set(r, (k = numbers.size));
    return k;
  });
  return module.spread(keys, gap).map((i) => items[i]);
}
