// Macros one level down (decision 0077, the owner: "One level down"): a macro maps its own level's modules' knobs and
// its devices' macros, never past a device's edge. A macro that reached into a device (a kept rack's Wild reached Calm's
// five thresholds) has those targets moved into a macro of that device, of its name, which it then maps whole (0 to 1),
// so it moves them exactly as before; deeper, the same at every level. Pure, in place: moved again, a patch is as it was.

import type { Module } from './patch';
import { isDevice, type PatchV2 } from './ports';

type Mapping = { id: string; label: string; value: number; targets: { module: string; knob: string; from: number; to: number }[] };

const freeId = (list: readonly Mapping[], base: string) => {
  let id = base;
  for (let n = 2; list.some((x) => x.id === id); n++) id = `${base}-${n}`;
  return id;
};

function level(macros: Mapping[], modules: readonly Module[]) {
  for (const x of macros) {
    const deep = x.targets.filter((t) => t.module.includes('/'));
    if (!deep.length) continue;
    const kept = x.targets.filter((t) => !t.module.includes('/'));
    const byDevice = new Map<string, typeof deep>();
    for (const t of deep) {
      const d = t.module.slice(0, t.module.indexOf('/'));
      (byDevice.get(d) ?? byDevice.set(d, []).get(d)!).push(t);
    }
    for (const [id, targets] of byDevice) {
      const d = modules.find((m) => m.id === id);
      if (!d || !isDevice(d)) {
        kept.push(...targets);
        continue;
      }
      let inner = d.macros.find((y) => y.id === x.id && y.label === x.label);
      if (!inner) {
        inner = { id: freeId(d.macros, x.id), label: x.label, value: x.value, targets: [] };
        d.macros.push(inner);
      }
      inner.targets.push(...targets.map((t) => ({ ...t, module: t.module.slice(id.length + 1) })));
      const knob = `macro:${inner.id}`;
      if (!kept.some((t) => t.module === id && t.knob === knob)) kept.push({ module: id, knob, from: 0, to: 1 });
    }
    x.targets = kept;
  }
  for (const m of modules) if (isDevice(m)) level(m.macros, m.patch.modules);
}

/** `p`'s macros each reaching one level down (in place), and `p`. */
export function macrosOneLevel(p: PatchV2): PatchV2 {
  level(p.macros, p.modules);
  return p;
}
