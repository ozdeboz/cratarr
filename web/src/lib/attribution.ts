// Why a list shows nothing, truly (decision 0095; the owner, 2026-11-02: "I want the real true attribution so that if
// the filters leave a beautiful list but the search narrows it to zero, then we know where to point at. And if the
// search has a list but the filter narrows it down, we should point to the filter"). Every thing the list holds is
// kept out by an exact set of causes (the server's `blocked`, the search one of them); lifting a set brings back
// everything kept out by no more than it. The answers are the smallest sets that bring anything back, each with how
// many; the one to point at is the smallest, the last change deciding between equals. Pure: no guessing, nothing but
// counting the sets that occur.

/** One answer: the causes to lift (indices into the causes) and how many things come back when they are. */
export type Answer = { causes: number[]; frees: number };

/** The sets that keep things out, with how many each keeps out exactly (a set as its sorted indices). */
export function countSets(groups: readonly { causes: readonly number[]; keys: readonly string[] }[]): Map<string, { causes: number[]; n: number }> {
  const out = new Map<string, { causes: number[]; n: number }>();
  for (const g of groups) {
    const causes = [...new Set(g.causes)].sort((a, b) => a - b);
    const id = causes.join(',');
    const was = out.get(id);
    if (was) was.n += g.keys.length;
    else out.set(id, { causes, n: g.keys.length });
  }
  return out;
}

const within = (a: readonly number[], b: readonly number[]) => a.every((x) => b.includes(x));

/** The smallest sets that bring anything back: no other set that occurs lies within one. Each with how many come back
 *  when it is lifted (everything kept out by it or by less), fewest causes first, then most brought back. */
export function answers(groups: readonly { causes: readonly number[]; keys: readonly string[] }[]): Answer[] {
  const sets = [...countSets(groups).values()].filter((s) => s.causes.length > 0);
  const least = sets.filter((s) => !sets.some((t) => t !== s && t.causes.length < s.causes.length && within(t.causes, s.causes)));
  return least
    .map((s) => ({ causes: s.causes, frees: sets.filter((t) => within(t.causes, s.causes)).reduce((n, t) => n + t.n, 0) }))
    .sort((a, b) => a.causes.length - b.causes.length || b.frees - a.frees);
}

/** The answer to point at: the fewest causes; among answers of as few, the one holding the cause changed last (what
 *  made the list empty in the listener's eyes); else the one bringing most back. None: nothing would come back. */
export function pointAt(found: readonly Answer[], last: number | null): Answer | null {
  if (!found.length) return null;
  const fewest = found.filter((a) => a.causes.length === found[0].causes.length);
  return (last !== null ? fewest.find((a) => a.causes.includes(last)) : undefined) ?? fewest[0];
}
