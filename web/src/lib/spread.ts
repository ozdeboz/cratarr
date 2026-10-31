// A record's tracks kept apart in a play order (0.12.1; the owner, 2026-10-31: a grabbed crate's "big play button had
// too much repetition of records in the queue"). The order given stays the base (a crate's closeness, a fair shuffle's
// draw); each step takes the first track left whose record has not come in the last `gap` tracks, `gap` being 4 or,
// where the tracks hold fewer records, one less than how many there are. When none may come (one record's tracks
// left), the one whose record came longest ago.

/** `items` in their order with each record's tracks at least `gap` other records apart where they can be. */
export function spreadRecords<T>(items: readonly T[], recordOf: (t: T) => string, gap = 4): T[] {
  const records = new Set(items.map(recordOf)).size;
  const keep = Math.max(0, Math.min(gap, records - 1));
  if (keep === 0 || items.length < 3) return [...items];
  const left = items.map((t) => ({ t, rec: recordOf(t) }));
  const last = new Map<string, number>();
  const out: T[] = [];
  while (left.length) {
    const step = out.length;
    let pick = left.findIndex((x) => {
      const at = last.get(x.rec);
      return at === undefined || step - at > keep;
    });
    if (pick < 0) {
      // None may come yet: the record heard longest ago, the first of its tracks left.
      let oldest = Infinity;
      left.forEach((x, k) => {
        const at = last.get(x.rec) ?? -1;
        if (at < oldest) [oldest, pick] = [at, k];
      });
    }
    const [x] = left.splice(pick, 1);
    last.set(x.rec, step);
    out.push(x.t);
  }
  return out;
}
