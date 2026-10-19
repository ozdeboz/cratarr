// The rack's groups, worked out from the wiring (decision 0057 §4: places are views, not structure): the sources, the
// picture's chain in the order pictures flow, the control (modules that move knobs), the output. The graph's lanes can
// use the same. Pure.

import { portsOf, type PatchV2, type Ports } from './ports';
import type { Module, ModuleType } from './patch';

export type Lane = 'sources' | 'picture' | 'control' | 'output';
export type Lanes = Record<Lane, Module[]>;

const PICTURE = new Set(['picture', 'light']);

/** Where a module stands, by its ports: a source gives a picture and takes none (the flame takes only its own last
 *  light: `source`); a picture module takes and gives one; the output takes one and gives none; the rest move knobs
 *  (control). */
export function laneOf(ports: Ports, source = false): Lane {
  if (source) return 'sources';
  const takes = ports.in.some((p) => PICTURE.has(p.type));
  const gives = ports.out.some((p) => PICTURE.has(p.type));
  if (gives && !takes) return 'sources';
  if (gives && takes) return 'picture';
  if (takes) return 'output';
  return 'control';
}

/** A patch's modules by lane; the picture lane in the order pictures flow (a module after what feeds it), ties and
 *  unwired ones as the patch has them. `portsFor`: a module's ports (a device's from what it exposes). */
export function lanesOf(p: PatchV2, portsFor: (m: Module) => Ports | null, isSource: (m: Module) => boolean = () => false): Lanes {
  const lanes: Lanes = { sources: [], picture: [], control: [], output: [] };
  for (const m of p.modules) {
    const ports = portsFor(m);
    if (ports) lanes[laneOf(ports, isSource(m))].push(m);
  }
  // The picture lane in flow order: each module after the picture modules feeding it (Kahn's order; a Feedback's
  // input, its last frame, does not count).
  const inLane = new Set(lanes.picture.map((m) => m.id));
  const before = new Map(lanes.picture.map((m) => [m.id, new Set<string>()]));
  for (const c of p.cables) {
    if (!inLane.has(c.from.module) || !inLane.has(c.to.module) || c.from.module === c.to.module) continue;
    if (p.modules.find((m) => m.id === c.to.module)?.type === 'feedback') continue;
    before.get(c.to.module)!.add(c.from.module);
  }
  const order: Module[] = [];
  const left = [...lanes.picture];
  while (left.length) {
    const i = left.findIndex((m) => [...before.get(m.id)!].every((id) => order.some((x) => x.id === id)));
    order.push(...left.splice(i < 0 ? 0 : i, 1));
  }
  lanes.picture = order;
  return lanes;
}

/** A module's ports from its type, or a device's from what it exposes. */
export function portsFrom(types: Record<string, ModuleType>) {
  return (m: Module): Ports | null => {
    if (m.type === 'device') {
      const d = m as Module & { inputs?: { as: string; label: string; module: string; port: string }[]; outputs?: { as: string; label: string; module: string; port: string }[]; patch?: PatchV2 };
      const inner = (id: string, port: string, dir: 'in' | 'out') => {
        const im = d.patch?.modules.find((x) => x.id === id);
        const t = im ? types[im.type] : undefined;
        return t ? portsOf(t)[dir].find((x) => x.id === port) : undefined;
      };
      return {
        in: (d.inputs ?? []).map((e) => ({ ...(inner(e.module, e.port, 'in') ?? { type: 'value' as const }), id: e.as, label: e.label })),
        out: (d.outputs ?? []).map((e) => ({ ...(inner(e.module, e.port, 'out') ?? { type: 'picture' as const }), id: e.as, label: e.label })),
      };
    }
    const t = types[m.type];
    return t ? portsOf(t) : null;
  };
}
