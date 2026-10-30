// The panel's edits of a v2 patch (decision 0057, the switch, S3), beside the graph's (patchgraph.ts `applyV2`: a
// knob, a connection, cables removed, a category pinned, a module added, a device made or unmade), which it hands on.
// Its own: a cable's amount, depth and channel; a cable's end moved (Reroute); a module removed with its cables, pins
// and macro targets; a pin of a knob, a module or a cable. Each at a level: `scope`, the devices it is inside. Pure: on
// a store's draft, in place.

import { applyV2, patchAt, type V2Edit } from './patchgraph';
import { isDevice, type CableV2, type Device, type PatchV2, type PortRef } from './ports';
import type { ModuleType } from './patch';
import { mapped } from './macro-map';

export type PanelEdit =
  | V2Edit
  | { kind: 'cable'; scope: string[]; id: string; value?: CableV2['value']; depth?: PortRef | null; channel?: number | null }
  | { kind: 'reroute'; scope: string[]; id: string; end: 'from' | 'to'; to: PortRef }
  | { kind: 'removeModule'; scope: string[]; id: string }
  | { kind: 'pinOne'; scope: string[]; pin: string; on: boolean }
  | { kind: 'addDevice'; scope: string[]; device: Device }
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
export function panelKey(e: PanelEdit): string | null {
  if (e.kind === 'knob') return `knob:${e.module.join('/')}.${e.knob}`;
  if (e.kind === 'macro') return `macro:${[...e.scope, e.id].join('/')}`;
  if (e.kind === 'macroRange') return `range:${[...e.scope, e.id].join('/')}.${e.module}.${e.knob}`;
  if (e.kind === 'cable') return `cable:${[...e.scope, e.id].join('/')}`;
  return null;
}

/** A macro edit (0077) applied to `p` in place, and `p`. One macro a knob: a knob mapped is taken from the macro that had
 *  it. Nothing jumps: a knob mapped has its range through where it is at the macro's value (its whole range wide); a
 *  knob let go, or a macro removed, leaves each knob where the macro had set it. */
function applyFace(p: PatchV2, e: Extract<PanelEdit, { kind: 'macro' | 'newMacro' | 'removeMacro' | 'mapKnob' | 'unmapKnob' | 'macroRange' }>, types?: Record<string, ModuleType>): PatchV2 {
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

/** An edit applied to `p` in place, and `p`. */
export function applyPanel(p: PatchV2, e: PanelEdit, types?: Record<string, ModuleType>): PatchV2 {
  if (e.kind === 'macro' || e.kind === 'newMacro' || e.kind === 'removeMacro' || e.kind === 'mapKnob' || e.kind === 'unmapKnob' || e.kind === 'macroRange') return applyFace(p, e, types);
  if (e.kind !== 'cable' && e.kind !== 'reroute' && e.kind !== 'removeModule' && e.kind !== 'pinOne' && e.kind !== 'addDevice') return applyV2(p, e, types);
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
  if (e.kind === 'pinOne') {
    inside.pins = inside.pins.filter((x) => x !== e.pin);
    if (e.on) inside.pins.push(e.pin);
    return p;
  }
  if (e.kind === 'removeModule') {
    // The Screen stays (its flash guard: 0057 §7e).
    if (inside.modules.find((m) => m.id === e.id)?.type === 'screen') return p;
    inside.modules = inside.modules.filter((m) => m.id !== e.id);
    const gone = inside.cables.filter((c) => c.from.module === e.id || c.to.module === e.id || c.depth?.module === e.id).map((c) => c.id);
    inside.cables = inside.cables.filter((c) => !gone.includes(c.id));
    inside.pins = inside.pins.filter((x) => !x.startsWith(`${e.id}.`) && x !== `module:${e.id}` && !gone.some((id) => x === `cable:${id}`));
    inside.macros = inside.macros.map((m) => ({ ...m, targets: m.targets.filter((t) => t.module !== e.id) }));
    return p;
  }
  const c = inside.cables.find((x) => x.id === e.id);
  if (!c) return p;
  if (e.kind === 'reroute') {
    c[e.end] = { ...e.to };
    return p;
  }
  if (e.value !== undefined) c.value = e.value;
  if (e.depth !== undefined) c.depth = e.depth;
  if (e.channel !== undefined) {
    const { channel: _, ...from } = c.from;
    c.from = e.channel === null ? from : { ...from, channel: e.channel };
  }
  return p;
}
