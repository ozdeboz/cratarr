// One head for every table (the owner, 2026-10-30: "Table header rows are inconsistent! Records and albums"): a list's
// column names are its head row (lib/Row `head`, decision 0042), and a name the list can be ordered by orders it on a
// tap, again the other way round, starting most (and newest) first where its order says `most` (decision 0060), its
// arrow on it as the list's header shows it (lib/ListHead). A name it cannot be ordered by is plain.
// Pure: the list's columns, its orders and the order it has in; the cells and title or count sorts for its head out.
import type { Cell, HeadSort } from './Row.svelte';
import type { Column } from './columns.svelte';

export type Order<K> = { key: K; down: boolean };
export type OrderOf<K> = { value: K; most?: true };

/** What a tap on the name of order `key` sets: it, starting most first where it says so; the same again turned round. */
export function tapped<K>(now: Order<K>, key: K, fields: readonly OrderOf<K>[]): Order<K> {
  return now.key === key ? { key, down: !now.down } : { key, down: !!fields.find((f) => f.value === key)?.most };
}

/** The head's column names: those the list orders by (`orderOf` a column's id to its order, or none) tap to order it. */
export function headCells<K>(cols: readonly Column[], orderOf: (id: string) => K | undefined, now: Order<K>, fields: readonly OrderOf<K>[], set: (o: Order<K>) => void): Cell[] {
  return cols.map((c) => {
    const key = orderOf(c.id);
    if (key === undefined || !fields.some((f) => f.value === key)) return { ...c, text: c.label };
    const on = now.key === key;
    return { ...c, text: c.label, on, down: on ? now.down : undefined, onclick: () => set(tapped(now, key, fields)) };
  });
}

/** The title's or the count's name as an order (Title, Name, Length, Tracks), where the list has that order. */
export function headSort<K>(key: K | undefined, now: Order<K>, fields: readonly OrderOf<K>[], set: (o: Order<K>) => void): HeadSort | undefined {
  if (key === undefined || !fields.some((f) => f.value === key)) return undefined;
  const on = now.key === key;
  return { on, down: on ? now.down : undefined, onclick: () => set(tapped(now, key, fields)) };
}
