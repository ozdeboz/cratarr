// The Library's composers (0.10, lib/classical), pure so the tests take them as they are.

/** A composer as Cratarr lists them (GET /api/classical/composers). */
export type Composer = { key: string; name: string; sort_name?: string | null; records: number; photo?: string | null };

/** By their sort name (Bach, Johann Sebastian), accents and case aside, as a library's index reads. */
export const byComposer = (a: Composer, b: Composer) =>
  (a.sort_name || a.name).localeCompare(b.sort_name || b.name, undefined, { sensitivity: 'base' }) || a.name.localeCompare(b.name);

/** The composers in the order chosen (lib/ComposerList): by name, or by how many records (fewest first; turned round,
 *  most first), those with as many by name; `down` turns it round. */
export function composersInOrder(list: readonly Composer[], order: 'name' | 'records', down: boolean): Composer[] {
  const way = down ? -1 : 1;
  const cmp = order === 'records' ? (a: Composer, b: Composer) => way * (a.records - b.records) || byComposer(a, b) : (a: Composer, b: Composer) => way * byComposer(a, b);
  return [...list].sort(cmp);
}
