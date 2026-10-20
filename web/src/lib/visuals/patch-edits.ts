// The panel's edits of a v2 patch (decision 0057, the switch, S3), beside the graph's (patchgraph.ts `applyV2`: a
// knob, a connection, cables removed, a category pinned, a module added, a device made or unmade), which it hands on.
// Its own: a cable's amount, depth and channel; a cable's end moved (Reroute); a module removed with its cables, pins
// and macro targets; a pin of a knob, a module or a cable. Each at a level: `scope`, the devices it is inside. Pure: on
// a store's draft, in place.

import { applyV2, patchAt, type V2Edit } from './patchgraph';
import type { CableV2, PatchV2, PortRef } from './ports';
import type { ModuleType } from './patch';

export type PanelEdit =
  | V2Edit
  | { kind: 'cable'; scope: string[]; id: string; value?: CableV2['value']; depth?: PortRef | null; channel?: number | null }
  | { kind: 'reroute'; scope: string[]; id: string; end: 'from' | 'to'; to: PortRef }
  | { kind: 'removeModule'; scope: string[]; id: string }
  | { kind: 'pinOne'; scope: string[]; pin: string; on: boolean };

/** The store's key for an edit: a knob's turns, a cable's curve drawn, one undo step each. */
export function panelKey(e: PanelEdit): string | null {
  if (e.kind === 'knob') return `knob:${e.module.join('/')}.${e.knob}`;
  if (e.kind === 'cable') return `cable:${[...e.scope, e.id].join('/')}`;
  return null;
}

/** An edit applied to `p` in place, and `p`. */
export function applyPanel(p: PatchV2, e: PanelEdit, types?: Record<string, ModuleType>): PatchV2 {
  if (e.kind !== 'cable' && e.kind !== 'reroute' && e.kind !== 'removeModule' && e.kind !== 'pinOne') return applyV2(p, e, types);
  const inside = patchAt(p, e.scope);
  if (!inside) return p;
  if (e.kind === 'pinOne') {
    inside.pins = inside.pins.filter((x) => x !== e.pin);
    if (e.on) inside.pins.push(e.pin);
    return p;
  }
  if (e.kind === 'removeModule') {
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
