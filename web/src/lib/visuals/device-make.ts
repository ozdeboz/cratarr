// Making a device of modules, and unmaking one (decision 0057 §6: "select modules, make a device, choose its macros").
// Pure, on one level of a v2 patch (the patch the modules are in):
// - make: the chosen modules move into a new device with the cables between them; each cable crossing the selection's
//   edge stays, led to an exposed port of the device (an input a port inside a cable comes into, an output one a cable
//   leaves from, each exposed once however many cables use it); the knobs chosen are its macros, each over its knob's
//   whole range from where the knob is now. The level's own macros that reach into it now reach the knob through the
//   device's path (`<device>/<module>`, as flatten names it). A device can be made inside a device.
// - unmake: the device's modules back at the level (an id already taken there gets a number), its cables with them,
//   the cables to and from its ports led back to the modules behind them, its macros the level's own.

import type { Knob, Module, ModuleType } from './patch';
import type { CableV2, Device, DeviceMacro, ExposedPort, PatchV2, PortRef } from './ports';
import { isDevice } from './ports';

/** A knob chosen as a macro: its module (in the selection), its knob, the macro's name. */
export type MacroPick = { module: string; knob: string; label?: string };

const free = (taken: ReadonlySet<string>, base: string) => {
  if (!taken.has(base)) return base;
  let n = 2;
  while (taken.has(`${base}-${n}`)) n++;
  return `${base}-${n}`;
};

/** A device made of the modules `ids` of `p` (in place: `p` is changed, a store's draft) with `id` and `label`;
 *  null where fewer than two of them are there. */
export function makeDevice(p: PatchV2, ids: readonly string[], o: { id?: string; label?: string; macros?: MacroPick[]; types?: Record<string, ModuleType> } = {}): Device | null {
  const inside = new Set(ids.filter((id) => p.modules.some((m) => m.id === id)));
  if (inside.size < 2) return null;
  const id = o.id ?? free(new Set(p.modules.map((m) => m.id)), 'device');
  const modules = p.modules.filter((m) => inside.has(m.id));
  const within = (r: PortRef) => inside.has(r.module);
  const cables: CableV2[] = [];
  const inputs: ExposedPort[] = [];
  const outputs: ExposedPort[] = [];
  const exposed = (list: ExposedPort[], r: PortRef): string => {
    const at = list.find((e) => e.module === r.module && e.port === r.port && e.channel === r.channel);
    if (at) return at.as;
    const as = free(new Set(list.map((e) => e.as)), r.port);
    const m = modules.find((x) => x.id === r.module);
    const knob = m && o.types?.[m.type]?.knobs.find((k) => k.id === r.port);
    list.push({ as, label: knob?.label ?? r.port.charAt(0).toUpperCase() + r.port.slice(1), module: r.module, port: r.port, ...(r.channel !== undefined ? { channel: r.channel } : {}) });
    return as;
  };
  const outside: CableV2[] = [];
  for (const c of p.cables) {
    const a = within(c.from);
    const b = within(c.to);
    if (a && b) cables.push(c);
    else if (b) outside.push({ ...c, to: { module: id, port: exposed(inputs, { module: c.to.module, port: c.to.port, channel: c.to.channel }) } });
    // Out of it: the output exposed whole; a channel taken of it, as the cable took it.
    else if (a) outside.push({ ...c, from: { ...c.from, module: id, port: exposed(outputs, { module: c.from.module, port: c.from.port }) } });
    else outside.push(c);
  }
  const knobOf = (m: Module, k: string): Knob | undefined => o.types?.[m.type]?.knobs.find((x) => x.id === k);
  const macros: DeviceMacro[] = (o.macros ?? []).flatMap((pick, i) => {
    const m = modules.find((x) => x.id === pick.module);
    if (!m) return [];
    const k = knobOf(m, pick.knob);
    const from = k?.min ?? 0;
    const to = k?.max ?? 1;
    const now = m.knobs[pick.knob] ?? k?.default ?? from;
    const value = to > from ? Math.min(1, Math.max(0, (now - from) / (to - from))) : 0;
    return [{ id: `macro-${i + 1}`, label: pick.label ?? k?.label ?? pick.knob, value, targets: [{ module: m.id, knob: pick.knob, from, to }] }];
  });
  const device: Device = { id, type: 'device', label: o.label ?? 'Device', knobs: {}, patch: { v: 2, modules, cables, macros: [], pins: [] }, inputs, outputs, macros };
  // The device where its first module was.
  const first = p.modules.findIndex((m) => inside.has(m.id));
  p.modules = p.modules.filter((m) => !inside.has(m.id));
  p.modules.splice(first, 0, device);
  p.cables = outside;
  for (const x of p.macros) for (const t of x.targets) if (inside.has(t.module)) t.module = `${id}/${t.module}`;
  p.pins = p.pins.map((pin) => {
    const dot = pin.indexOf('.');
    const m = dot < 0 ? pin : pin.slice(0, dot);
    return inside.has(m) ? `${id}/${pin}` : pin;
  });
  return device;
}

/** The device `id` of `p` unmade (in place); false where it is not one. */
export function unmakeDevice(p: PatchV2, id: string): boolean {
  const at = p.modules.findIndex((m) => m.id === id);
  const d = p.modules[at];
  if (!d || !isDevice(d)) return false;
  const taken = new Set(p.modules.filter((m) => m.id !== id).map((m) => m.id));
  const rename = new Map<string, string>();
  for (const m of d.patch.modules) {
    const next = free(taken, m.id);
    taken.add(next);
    rename.set(m.id, next);
  }
  const moved = d.patch.modules.map((m) => ({ ...m, id: rename.get(m.id)! }));
  const inner = (r: PortRef): PortRef => ({ ...r, module: rename.get(r.module) ?? r.module });
  const lead = (list: ExposedPort[], r: PortRef): PortRef => {
    if (r.module !== id) return r;
    const e = list.find((x) => x.as === r.port);
    return e ? { ...r, module: rename.get(e.module) ?? e.module, port: e.port, ...(e.channel !== undefined ? { channel: e.channel } : {}) } : r;
  };
  const cables = [
    ...p.cables.filter((c) => !((c.from.module === id && !d.outputs.some((e) => e.as === c.from.port)) || (c.to.module === id && !d.inputs.some((e) => e.as === c.to.port)))).map((c) => ({ ...c, from: lead(d.outputs, c.from), to: lead(d.inputs, c.to) })),
    ...d.patch.cables.map((c) => ({ ...c, id: free(new Set(p.cables.map((x) => x.id)), c.id), from: inner(c.from), to: inner(c.to) })),
  ];
  p.modules.splice(at, 1, ...moved);
  p.cables = cables;
  p.macros = [
    ...p.macros.map((x) => ({ ...x, targets: x.targets.map((t) => (t.module.startsWith(`${id}/`) ? { ...t, module: rename.get(t.module.slice(id.length + 1)) ?? t.module } : t)) })),
    ...d.macros.map((x) => ({ id: free(new Set(p.macros.map((y) => y.id)), x.id), on: id, label: x.label, value: x.value, targets: x.targets.map((t) => ({ ...t, module: rename.get(t.module) ?? t.module })) })),
  ];
  const renamed = (pin: string) => {
    const dot = pin.indexOf('.');
    const m = dot < 0 ? pin : pin.slice(0, dot);
    return rename.has(m) ? rename.get(m)! + pin.slice(m.length) : pin;
  };
  p.pins = [...p.pins.map((pin) => (pin.startsWith(`${id}/`) ? renamed(pin.slice(id.length + 1)) : pin)), ...d.patch.pins.map(renamed)];
  return true;
}
