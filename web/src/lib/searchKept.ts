// What a search keeps by its words (lib/globalSearch: the server's answers, Show all's): the newest few, the oldest
// let go first. Pure.
export const KEEP = 30;
export function keep<K, V>(m: Map<K, V>, k: K, v: V, most = KEEP) {
  m.delete(k);
  m.set(k, v);
  while (m.size > most) m.delete(m.keys().next().value as K);
}
