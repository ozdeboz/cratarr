// A playlist's edits as operations (decision 0087): add a track as a new entry after one, remove an entry, move an
// entry after one (null: to the start). Made where the edit is made, kept in the outbox, replayed by Cratarr and merged
// there: which entries are in it as a set (adding a known entry or removing one already gone changes nothing), the
// order as the last move made it. So a playlist of one's own can be edited with no network, here and elsewhere at once.

export type PlaylistOp =
  | { op_id: string; at: number; op: 'add'; entry: string; track: { source: string; id: string }; after: string | null }
  | { op_id: string; at: number; op: 'remove'; entry: string }
  | { op_id: string; at: number; op: 'move'; entry: string; after: string | null };

/** The operations that make `after` of `before` (both lists of entry ids; `after` holding only entries of `before`):
 *  the entries left out removed, then the fewest moves. The entries that keep their order among themselves (the
 *  longest such run) stay; each other is moved after the one before it in `after`, in `after`'s order, so the one it
 *  follows is in place by then. */
export function opsFor(before: readonly string[], after: readonly string[], now: number, newId: () => string): PlaylistOp[] {
  const kept = new Set(after);
  const ops: PlaylistOp[] = before.filter((e) => !kept.has(e)).map((entry) => ({ op_id: newId(), at: now, op: 'remove', entry }));
  const place = new Map(before.map((e, i) => [e, i]));
  const stay = longestRun(after.map((e) => place.get(e) ?? -1));
  after.forEach((entry, i) => {
    if (stay.has(i)) return;
    ops.push({ op_id: newId(), at: now, op: 'move', entry, after: i ? after[i - 1] : null });
  });
  return ops;
}

/** The positions (in `xs`) of one longest strictly rising run of `xs`. */
function longestRun(xs: readonly number[]): Set<number> {
  const tails: number[] = [];
  const back: number[] = new Array(xs.length).fill(-1);
  for (let i = 0; i < xs.length; i++) {
    if (xs[i] < 0) continue;
    let lo = 0;
    let hi = tails.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (xs[tails[mid]] < xs[i]) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0) back[i] = tails[lo - 1];
    tails[lo] = i;
  }
  const run = new Set<number>();
  for (let i = tails.length ? tails[tails.length - 1] : -1; i >= 0; i = back[i]) run.add(i);
  return run;
}

/** The entries as the operations leave them, applied in order, as Cratarr merges them: an add of an entry there already,
 *  a remove or a move of one not there, changes nothing; an `after` not there means the end. */
export function applyOps<T>(entries: readonly T[], ops: readonly PlaylistOp[], idOf: (x: T) => string, made: (op: Extract<PlaylistOp, { op: 'add' }>) => T): T[] {
  const list = [...entries];
  const at = (entry: string) => list.findIndex((x) => idOf(x) === entry);
  const put = (x: T, after: string | null) => {
    if (after === null) return void list.unshift(x);
    const i = at(after);
    if (i < 0) list.push(x);
    else list.splice(i + 1, 0, x);
  };
  for (const op of ops) {
    const i = at(op.entry);
    if (op.op === 'add') {
      if (i < 0) put(made(op), op.after);
    } else if (op.op === 'remove') {
      if (i >= 0) list.splice(i, 1);
    } else if (i >= 0) {
      const [x] = list.splice(i, 1);
      put(x, op.after);
    }
  }
  return list;
}
