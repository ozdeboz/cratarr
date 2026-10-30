// The Loudness Follow's VCAs (0057 §7f, toV2 `followed`: the loudness scaling every cable, in sight): a VCA keyed by
// the Loudness's `out` with one cable in. What reads a cable's source as the signal it carries (the fluid's push
// gains, the Cables roll) reads through one to the cable into it.

import type { CableV2, PatchV2 } from '../ports';

const kept = new WeakMap<PatchV2, Map<string, CableV2>>();

/** Each follow VCA of `p` (flattened), by id: the one cable into it. */
export function followsOf(p: PatchV2): Map<string, CableV2> {
  let out = kept.get(p);
  if (out) return out;
  out = new Map();
  const typeOf = new Map(p.modules.map((m) => [m.id, m.type]));
  const into = new Map<string, CableV2[]>();
  for (const c of p.cables) if (typeOf.get(c.to.module) === 'vca') (into.get(c.to.module) ?? into.set(c.to.module, []).get(c.to.module)!).push(c);
  for (const [id, cs] of into) {
    const ins = cs.filter((c) => c.to.port === 'in');
    const keyed = cs.some((c) => c.to.port === 'cv' && typeOf.get(c.from.module) === 'envelope' && c.from.port === 'out');
    if (keyed && ins.length === 1 && cs.length === 2) out.set(id, ins[0]);
  }
  kept.set(p, out);
  return out;
}
