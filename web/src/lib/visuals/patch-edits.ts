// The rack's edits, all of them through one reducer (0.10, the architecture audit: "one edit reducer"): the
// graph's (patchgraph.ts `applyV2`: a knob, a connection, cables removed, a cable's end moved, a pin, modules removed,
// a category pinned, a module added, a device made or unmade), the panel's (a cable's amount, depth and channel; a
// module removed; a device added; a level's macros: decision 0077) and the visuals' own (the values a roll or a step's
// Next moves). One way per kind: a module removed by the panel goes as the graph removes them. Each at a level:
// `scope`, the devices it is inside. Pure: on a store's draft, in place.

import { applyV2, patchAt, type V2Edit } from './patchgraph';
import { isDevice, type CableV2, type Device, type PatchV2, type PortRef } from './ports';
import type { ModuleType } from './patch';
import { mapped } from './macro-map';
import { applyKnobs, applyMacros } from './v2';

export type RackEdit =
  | V2Edit
  | { kind: 'cable'; scope: string[]; id: string; value?: CableV2['value']; depth?: PortRef | null; channel?: number | null }
  | { kind: 'removeModule'; scope: string[]; id: string }
  | { kind: 'addDevice'; scope: string[]; device: Device }
  // The values the visuals move as they play or roll (a roll, a new track's look, a step's Next), by module id as the
  // engine runs them (a device's inner ones `<device>/<module>`), cables by id, macros by their node's id.
  | { kind: 'values'; knobs: Record<string, Record<string, number>>; cables?: Record<string, CableV2['value']>; macros?: Record<string, number> }
  | { kind: 'macro'; scope: string[]; id: string; value: number }
  // A level's macros (0077: its face; the rack's at the top, a device's own inside it): one made, removed, a knob of the
  // level's (or a device's macro, `macro:<id>`) mapped, let go, its range changed.
  | { kind: 'newMacro'; scope: string[]; id: string; label: string }
  | { kind: 'removeMacro'; scope: string[]; id: string }
  | { kind: 'mapKnob'; scope: string[]; id: string; module: string; knob: string }
  | { kind: 'unmapKnob'; scope: string[]; id: string; module: string; knob: string }
  | { kind: 'macroRange'; scope: string[]; id: string; module: string; knob: string; from?: number; to?: number };

type Mapping = { id: string; label: string; value: number; targets: { module: string; knob: string; from: number; to: number }[] };

/** A level's macros, its face (0077): the rack's at the top, a device's own inside it; and the level they map. */
export function faceAt(p: PatchV2, scope: readonly string[]): { macros: Mapping[]; level: PatchV2 } | null {
  if (!scope.length) return { macros: p.macros, level: p };
  const d = patchAt(p, scope.slice(0, -1))?.modules.find((m) => m.id === scope[scope.length - 1]);
  return d && isDevice(d) ? { macros: d.macros, level: d.patch } : null;
}

/** A free id for a new macro at a level (`macro-<n>`), and its name (`Macro <n>`). */
export function freeMacro(macros: readonly { id: string }[]): { id: string; label: string } {
  let n = macros.length + 1;
  while (macros.some((x) => x.id === `macro-${n}`)) n++;
  return { id: `macro-${n}`, label: `Macro ${n}` };
}

/** What a target of a macro is set to at the macro's value (along the knob's taper: the one rule, macro-map.ts). */
const setTo = mapped;

/** The store's key for an edit: a knob's turns, a cable's curve drawn, one undo step each. */
export function panelKey(e: RackEdit): string | null {
  if (e.kind === 'knob') return `knob:${e.module.join('/')}.${e.knob}`;
  if (e.kind === 'macro') return `macro:${[...e.scope, e.id].join('/')}`;
  if (e.kind === 'macroRange') return `range:${[...e.scope, e.id].join('/')}.${e.module}.${e.knob}`;
  if (e.kind === 'cable') return `cable:${[...e.scope, e.id].join('/')}`;
  return null;
}

/** A macro edit (0077) applied to `p` in place, and `p`. One macro a knob: a knob mapped is taken from the macro that had
 *  it. Nothing jumps: a knob mapped has its range through where it is at the macro's value (its whole range wide); a
 *  knob let go, or a macro removed, leaves each knob where the macro had set it. */
function applyFace(p: PatchV2, e: Extract<RackEdit, { kind: 'macro' | 'newMacro' | 'removeMacro' | 'mapKnob' | 'unmapKnob' | 'macroRange' }>, types?: Record<string, ModuleType>): PatchV2 {
  const face = faceAt(p, e.scope);
  if (!face) return p;
  const { macros, level } = face;
  const x = macros.find((y) => y.id === e.id);
  const knobOf = (module: string, knob: string) => {
    const m = level.modules.find((y) => y.id === module);
    return m && !isDevice(m) ? types?.[m.type]?.knobs.find((k) => k.id === knob.replace(/\.\d+$/, '')) : undefined;
  };
  /** Where a target is now: as its macro sets it, else its own value (a device's macro: its value). */
  const now = (module: string, knob: string) => {
    for (const y of macros) {
      const t = y.targets.find((u) => u.module === module && u.knob === knob);
      if (t) return setTo(t, Math.min(1, Math.max(0, y.value)), knobOf(module, knob)?.taper === 'exp');
    }
    const m = level.modules.find((y) => y.id === module);
    if (m && isDevice(m) && knob.startsWith('macro:')) return m.macros.find((y) => y.id === knob.slice('macro:'.length))?.value ?? 0.5;
    return m?.knobs[knob] ?? knobOf(module, knob)?.default ?? 0;
  };
  /** A target let go: left where its macro had set it. */
  const keep = (module: string, knob: string) => {
    const v = now(module, knob);
    const m = level.modules.find((y) => y.id === module);
    if (!m) return;
    if (isDevice(m) && knob.startsWith('macro:')) {
      const inner = m.macros.find((y) => y.id === knob.slice('macro:'.length));
      if (inner) inner.value = Math.min(1, Math.max(0, v));
    } else m.knobs[knob] = v;
  };
  if (e.kind === 'newMacro') {
    if (macros.some((y) => y.id === e.id)) return p;
    macros.push({ id: e.id, label: e.label, value: 0.5, targets: [], ...(e.scope.length ? {} : { on: 'rack' }) } as Mapping);
    return p;
  }
  if (!x) return p;
  if (e.kind === 'macro') {
    x.value = Math.min(1, Math.max(0, e.value));
    return p;
  }
  if (e.kind === 'removeMacro') {
    for (const t of x.targets) keep(t.module, t.knob);
    macros.splice(macros.indexOf(x), 1);
    level.pins = level.pins.filter((y) => y !== `macro:${e.id}`);
    return p;
  }
  if (e.kind === 'unmapKnob') {
    if (!x.targets.some((t) => t.module === e.module && t.knob === e.knob)) return p;
    keep(e.module, e.knob);
    x.targets = x.targets.filter((t) => !(t.module === e.module && t.knob === e.knob));
    return p;
  }
  if (e.kind === 'macroRange') {
    const t = x.targets.find((u) => u.module === e.module && u.knob === e.knob);
    if (t && e.from !== undefined) t.from = e.from;
    if (t && e.to !== undefined) t.to = e.to;
    return p;
  }
  // mapKnob: its whole range wide, through where it is now at this macro's value; taken from any other.
  const k = knobOf(e.module, e.knob);
  const lo = k ? k.min : 0;
  const hi = k ? k.max : 1;
  const v = Math.min(1, Math.max(0, x.value));
  const at = now(e.module, e.knob);
  for (const y of macros) y.targets = y.targets.filter((t) => !(t.module === e.module && t.knob === e.knob));
  const from = at - (hi - lo) * v;
  x.targets.push({ module: e.module, knob: e.knob, from, to: from + (hi - lo) });
  return p;
}

/** An edit applied to `p` in place, and `p`: the one way the rack is changed. */
export function applyEdit(p: PatchV2, e: RackEdit, types?: Record<string, ModuleType>): PatchV2 {
  if (e.kind === 'macro' || e.kind === 'newMacro' || e.kind === 'removeMacro' || e.kind === 'mapKnob' || e.kind === 'unmapKnob' || e.kind === 'macroRange') return applyFace(p, e, types);
  if (e.kind === 'values') {
    applyKnobs(p, e.knobs);
    for (const c of p.cables) if (e.cables && c.id in e.cables) c.value = e.cables[c.id];
    applyMacros(p, e.macros ?? {});
    return p;
  }
  // A module removed as the graph removes them (its cables, its pins, its macros' targets, a device's ports to it);
  // never the Screen (its flash guard: 0057 §7e).
  if (e.kind === 'removeModule') return patchAt(p, e.scope)?.modules.find((m) => m.id === e.id)?.type === 'screen' ? p : applyV2(p, { kind: 'removeModules', scope: e.scope, modules: [e.id] }, types);
  if (e.kind !== 'cable' && e.kind !== 'addDevice') return applyV2(p, e, types);
  const inside = patchAt(p, e.scope);
  if (!inside) return p;
  if (e.kind === 'addDevice') {
    // A copy, its id free at the level (`<device>`, else `<device>-2` …).
    const taken = new Set(inside.modules.map((m) => m.id));
    let id = e.device.id;
    for (let n = 2; taken.has(id); n++) id = `${e.device.id}-${n}`;
    inside.modules.push({ ...structuredClone(e.device), id });
    return p;
  }
  const c = inside.cables.find((x) => x.id === e.id);
  if (!c) return p;
  if (e.value !== undefined) c.value = e.value;
  if (e.depth !== undefined) c.depth = e.depth;
  if (e.channel !== undefined) {
    const { channel: _, ...from } = c.from;
    c.from = e.channel === null ? from : { ...from, channel: e.channel };
  }
  return p;
}
