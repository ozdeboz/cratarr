// The patch drawn as a graph (decision 0056, step 3): PatchGraph's nodes and edges from a patch, its module types and
// where things were put. Picture cables follow the patch's order (each layer's source, its modules, its own loop, into
// the blend; then the loop, finish, the screen); modulation cables are the patch's cables, from a signal or a modulator
// to a knob or a modulator's port. Positions are not the patch's (0056 keeps none): the parent keeps them by graph id
// and anything without one is laid out here. Pure.

import { endOf, type RouteValue } from './curves';
import type { Cable, Knob, Module, ModuleType, Patch, Place, SignalRef } from './patch';
import { SIGNAL_LABEL, type Signal } from './routes';

export type Side = 'left' | 'right' | 'top' | 'bottom';
export type XY = { x: number; y: number };
export type Size = { width: number; height: number };

/** A knob as drawn: its value and where that sits in its range (0..1), and the stretch its cables move it over. */
export type KnobView = {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  kind: Knob['kind'];
  at: number;
  ring: [number, number] | null;
};

export type ModuleData = {
  kind: 'module';
  module: string;
  label: string;
  place: Place;
  knobs: KnobView[];
  /** Knobs not drawn until the node is opened out. */
  hidden: number;
  /** A modulator's input ports, and whether anything is cabled into each. */
  ports: { id: string; cabled: boolean }[];
  /** Where the picture comes in and goes out (none: no picture). */
  in: Side | null;
  out: Side | null;
  /** A loop's last module: its picture also goes back to the loop's first. */
  feedback: boolean;
  modulator: boolean;
};
export type SignalData = { kind: 'signal'; label: string; ref: SignalRef };
export type MarkData = { kind: 'blend' | 'screen'; label: string; in: Side; out: Side | null };
export type BoxData = { kind: 'box'; label: string };
export type NodeData = ModuleData | SignalData | MarkData | BoxData;

export type GraphNode = {
  id: string;
  type: 'patch' | 'box';
  position: XY;
  data: NodeData;
  parentId?: string;
  extent?: 'parent';
  expandParent?: boolean;
  width?: number;
  height?: number;
  selectable?: boolean;
  connectable?: boolean;
  deletable: false;
  zIndex?: number;
};

export type GraphEdge = {
  id: string;
  source: string;
  target: string;
  sourceHandle: string;
  targetHandle: string;
  type?: 'cable' | 'smoothstep';
  class: 'picture' | 'feedback' | 'cable';
  label?: string;
  deletable: boolean;
  selectable: boolean;
  data?: { amount: string };
};

/** Graph ids that are not a module's (a module's node is its id). */
export const BLEND = '@blend';
export const SCREEN = '@screen';
export const LOOP = '@loop';
export const layerBox = (layer: string) => `layer:${layer}`;
export const signalKey = (r: SignalRef) => `signal:${r.signal}|${r.band ?? '-'}|${r.reference ?? 'track'}|${r.speed ?? 'fast'}`;
/** A knob's (or a port's) handle on its module's node. */
export const knobHandle = (knob: string) => `k:${knob}`;
export const MOD_OUT = 'mod';

const KNOB_W = 44;
const KNOBS_ACROSS = 6;
/** How many knobs a module shows before it is opened out (its cabled ones always); a modulator shows its cabled ones. */
const SHOWN = 3;
const GAP_X = 64;
const GAP_Y = 28;
const PAD = 16;
const BOX_TOP = 30;
const ROW_ACROSS = 6;

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

/** A signal's name as a node shows it: its label, its band, slow, its reference where not this track. */
export function signalLabel(r: SignalRef): string {
  const parts = [SIGNAL_LABEL[r.signal as Signal] ?? r.signal];
  if (r.band !== undefined) parts.push(`${r.band + 1}`);
  if (r.speed === 'slow') parts.push('slow');
  if (r.reference && r.reference !== 'track') parts.push(r.reference);
  return parts.join(' · ');
}

/** A cable's amount as its label shows it: a swing signed, a level as a share. */
export function amountLabel(value: RouteValue, mode: Cable['mode']): string {
  const pc = Math.round(endOf(value) * 100);
  return mode === 'swing' && pc > 0 ? `+${pc}%` : `${pc}%`.replace('-', '−');
}

/** The stretch of a knob's range (0..1) its cables move it over: a swing's amount above (or below) where it is set, a
 *  level's share under it. Amounts are of the knob's whole range; none where nothing is cabled. */
export function ringOf(at: number, cables: Pick<Cable, 'value' | 'mode'>[]): [number, number] | null {
  if (!cables.length) return null;
  let up = 0;
  let down = 0;
  let share = 0;
  for (const c of cables) {
    const a = endOf(c.value);
    if (c.mode === 'level') share += Math.abs(a);
    else if (a > 0) up += a;
    else down += a;
  }
  return [clamp01(at * (1 - clamp01(share)) + down), clamp01(at + up)];
}

/** A module's type, or one made up from its knobs where its type is not known. */
function typeOf(m: Module, types: Record<string, ModuleType>): ModuleType {
  return (
    types[m.type] ?? {
      type: m.type,
      label: m.type,
      place: 'module',
      knobs: Object.keys(m.knobs).map((id) => ({ id, label: id, min: 0, max: 1, neutral: 0, default: 0, kind: 'level' as const })),
    }
  );
}

/** A node's size before it is measured, from what it shows. */
export function sizeOf(d: NodeData): Size {
  if (d.kind !== 'module') return d.kind === 'box' ? { width: 0, height: 0 } : { width: d.kind === 'signal' ? 150 : d.kind === 'blend' ? 84 : 120, height: 36 };
  const shown = d.knobs.length + (d.hidden ? 1 : 0);
  const rows = Math.ceil(shown / KNOBS_ACROSS);
  return {
    width: Math.max(d.modulator ? 150 : 140, 20 + Math.min(shown, KNOBS_ACROSS) * KNOB_W),
    height: 38 + rows * 50 + d.ports.length * 20,
  };
}

export type GraphInput = {
  patch: Patch;
  types: Record<string, ModuleType>;
  /** Where things were put, by graph id; a module in a box relative to its box. */
  positions?: Record<string, XY>;
  /** Signals to show though nothing is cabled from them yet. */
  signals?: SignalRef[];
  /** Nodes opened out to every knob. */
  open?: ReadonlySet<string>;
  /** Nodes' sizes as measured, where they have been. */
  sizes?: Record<string, Size>;
};

/** The patch as Svelte Flow's nodes and edges. */
export function graphOf({ patch, types, positions = {}, signals = [], open = new Set(), sizes = {} }: GraphInput): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const into = new Map<string, Cable[]>();
  for (const c of patch.cables) {
    const key = `${c.to.module}.${c.to.knob}`;
    (into.get(key) ?? into.set(key, []).get(key)!).push(c);
  }
  const size = (n: GraphNode) => sizes[n.id] ?? sizeOf(n.data);
  const put = (id: string, auto: XY) => positions[id] ?? auto;

  const moduleData = (m: Module, flow: { in: Side | null; out: Side | null; feedback?: boolean }): ModuleData => {
    const t = typeOf(m, types);
    const modulator = t.place === 'modulator';
    const all = t.knobs.filter((k) => !k.device);
    const cabled = (k: string) => into.has(`${m.id}.${k}`);
    const opened = open.has(m.id);
    let room = modulator ? 0 : SHOWN;
    const shown = all.filter((k) => {
      if (opened || cabled(k.id)) return true;
      return room-- > 0;
    });
    // Shown in the type's order, cabled or not.
    const knobs: KnobView[] = shown.map((k) => {
      const value = m.knobs[k.id] ?? k.default;
      const at = k.max > k.min ? clamp01((value - k.min) / (k.max - k.min)) : 0;
      return { id: k.id, label: k.label, value, min: k.min, max: k.max, kind: k.kind, at, ring: ringOf(at, into.get(`${m.id}.${k.id}`) ?? []) };
    });
    return {
      kind: 'module',
      module: m.id,
      label: t.label,
      place: t.place,
      knobs,
      hidden: all.length - shown.length,
      ports: (t.inputs ?? []).map((p) => ({ id: p, cabled: cabled(p) })),
      in: flow.in,
      out: flow.out,
      feedback: !!flow.feedback,
      modulator,
    };
  };

  const node = (id: string, data: NodeData, auto: XY, parent?: string): GraphNode => {
    const n: GraphNode = { id, type: 'patch', position: put(id, auto), data, deletable: false };
    if (parent) Object.assign(n, { parentId: parent, extent: 'parent', expandParent: true });
    if (data.kind !== 'module') n.connectable = data.kind === 'signal';
    nodes.push(n);
    return n;
  };
  const picture = (a: string, b: string, aHandle = 'out', label?: string) =>
    edges.push({ id: `pic:${a}>${b}`, source: a, target: b, sourceHandle: aHandle, targetHandle: 'in', class: 'picture', deletable: false, selectable: false, ...(label ? { label } : {}) });

  /** A box around a chain: its row of modules, then a column (a loop), each placed inside it. Returns the box's node
   *  and its last module's id. */
  const chain = (boxId: string, label: string, row: Module[], column: Module[], first: { in: Side | null }, origin: XY) => {
    const box: GraphNode = { id: boxId, type: 'box', position: put(boxId, origin), data: { kind: 'box', label }, deletable: false, selectable: false, connectable: false, zIndex: -1 };
    nodes.push(box);
    const inside: GraphNode[] = [];
    let x = PAD;
    row.forEach((m, i) => {
      const flow = { in: i === 0 ? first.in : ('left' as Side), out: 'right' as Side };
      const n = node(m.id, moduleData(m, flow), { x, y: BOX_TOP }, boxId);
      inside.push(n);
      x += size(n).width + GAP_X;
    });
    let y = BOX_TOP;
    column.forEach((m, i) => {
      const last = i === column.length - 1;
      const flow = {
        in: i === 0 ? (row.length ? 'left' : first.in) : ('top' as Side),
        out: last ? ('right' as Side) : ('bottom' as Side),
        feedback: last && column.length > 1,
      };
      const n = node(m.id, moduleData(m, flow), { x, y }, boxId);
      inside.push(n);
      y += size(n).height + GAP_Y;
    });
    const all = [...row, ...column];
    for (let i = 1; i < all.length; i++) picture(all[i - 1].id, all[i].id);
    if (column.length > 1)
      edges.push({ id: `fb:${column.at(-1)!.id}>${column[0].id}`, source: column.at(-1)!.id, target: column[0].id, sourceHandle: 'fb', targetHandle: 'in', type: 'smoothstep', class: 'feedback', label: 'feedback', deletable: false, selectable: false });
    box.width = Math.max(120, ...inside.map((n) => n.position.x + size(n).width)) + PAD;
    box.height = Math.max(60, ...inside.map((n) => n.position.y + size(n).height)) + PAD;
    return { box, last: all.at(-1)?.id ?? null };
  };

  // The layers, one box each, stacked.
  let y = 0;
  let right = 0;
  const toBlend: { id: string; label?: string }[] = [];
  for (const l of patch.layers) {
    const { box, last } = chain(layerBox(l.id), typeOf(l.source, types).label, [l.source, ...l.modules], l.loop, { in: null }, { x: 0, y });
    y = box.position.y + box.height! + 40;
    right = Math.max(right, box.position.x + box.width!);
    const b = l.blend;
    if (last) toBlend.push({ id: last, label: b.mode === 'add' && b.amount === 1 ? undefined : `${b.mode} ${Math.round(b.amount * 100)}%` });
  }
  const layersBottom = y;

  // The blend, the picture's loop, finish and the screen.
  const blendData: MarkData = { kind: 'blend', label: 'Blend', in: 'left', out: 'right' };
  const blend = node(BLEND, blendData, { x: right + GAP_X, y: Math.max(0, (layersBottom - 40) / 2 - 18) });
  for (const f of toBlend) picture(f.id, BLEND, 'out', f.label);
  let from = BLEND;
  let bottom = layersBottom;
  let x = blend.position.x + size(blend).width + GAP_X;
  if (patch.loop.length) {
    const { box, last } = chain(LOOP, 'Loop', [], patch.loop, { in: 'left' }, { x, y: Math.max(0, blend.position.y - BOX_TOP - 4) });
    picture(BLEND, patch.loop[0].id);
    from = last!;
    x = box.position.x + box.width! + GAP_X;
    bottom = Math.max(bottom, box.position.y + box.height!);
  }
  let fy = Math.max(0, blend.position.y - 4);
  let wide = 120;
  patch.finish.forEach((m, i) => {
    const n = node(m.id, moduleData(m, { in: i === 0 ? 'left' : 'top', out: 'bottom' }), { x, y: fy });
    picture(from, m.id);
    from = m.id;
    fy = n.position.y + size(n).height + GAP_Y;
    wide = Math.max(wide, size(n).width);
  });
  const screenData: MarkData = { kind: 'screen', label: 'Screen', in: patch.finish.length ? 'top' : 'left', out: null };
  const screen = node(SCREEN, screenData, { x, y: fy });
  picture(from, SCREEN);
  bottom = Math.max(bottom, screen.position.y + 36);

  // The whole frame: knobs, no picture.
  const frame = node(patch.frame.id, moduleData(patch.frame, { in: null, out: null }), { x: x + wide + GAP_X, y: Math.max(0, blend.position.y - 4) });
  bottom = Math.max(bottom, frame.position.y + size(frame).height);

  // Signals and modulators along the bottom.
  const sources: { id: string; data: NodeData }[] = [];
  const seen = new Set<string>();
  for (const r of [...patch.cables.flatMap((c) => ('modulator' in c.from ? [] : [c.from])), ...signals]) {
    const id = signalKey(r);
    if (seen.has(id)) continue;
    seen.add(id);
    sources.push({ id, data: { kind: 'signal', label: signalLabel(r), ref: r } });
  }
  for (const m of patch.modulators) sources.push({ id: m.id, data: moduleData(m, { in: null, out: null }) });
  let sy = bottom + 140;
  for (let i = 0; i < sources.length; i += ROW_ACROSS) {
    let sx = 0;
    let rowH = 0;
    for (const s of sources.slice(i, i + ROW_ACROSS)) {
      const n = node(s.id, s.data, { x: sx, y: sy });
      sx += Math.max(190, size(n).width + 40);
      rowH = Math.max(rowH, size(n).height);
    }
    sy += rowH + 60;
  }

  // The modulation cables, where both ends are drawn.
  const ids = new Set(nodes.map((n) => n.id));
  const modules = new Map(nodes.filter((n) => n.data.kind === 'module').map((n) => [n.id, n.data as ModuleData]));
  for (const c of patch.cables) {
    const source = 'modulator' in c.from ? c.from.modulator : signalKey(c.from);
    const target = modules.get(c.to.module);
    if (!ids.has(source) || !target) continue;
    if (!target.knobs.some((k) => k.id === c.to.knob) && !target.ports.some((p) => p.id === c.to.knob)) continue;
    const amount = amountLabel(c.value, c.mode);
    edges.push({ id: c.id, source, target: c.to.module, sourceHandle: MOD_OUT, targetHandle: knobHandle(c.to.knob), type: 'cable', class: 'cable', deletable: true, selectable: true, data: { amount } });
  }
  return { nodes, edges };
}

/** What a connection drawn in the graph cables, where it is one: from a signal's or a modulator's output to a knob or
 *  a port of another module (not one already cabled from there). */
export function cableOf(
  c: { source: string; target: string; sourceHandle?: string | null; targetHandle?: string | null },
  nodes: { id: string; data: NodeData }[],
  cables: Cable[],
): { from: Cable['from']; to: Cable['to'] } | null {
  if (c.sourceHandle !== MOD_OUT || !c.targetHandle?.startsWith('k:') || c.source === c.target) return null;
  const src = nodes.find((n) => n.id === c.source)?.data;
  const dst = nodes.find((n) => n.id === c.target)?.data;
  if (!src || dst?.kind !== 'module') return null;
  const from: Cable['from'] | null = src.kind === 'signal' ? src.ref : src.kind === 'module' && src.modulator ? { modulator: src.module } : null;
  if (!from) return null;
  const to = { module: dst.module, knob: c.targetHandle.slice(2) };
  const same = (f: Cable['from']) => ('modulator' in f && 'modulator' in from ? f.modulator === from.modulator : !('modulator' in f) && !('modulator' in from) && signalKey(f) === signalKey(from));
  if (cables.some((x) => x.to.module === to.module && x.to.knob === to.knob && same(x.from))) return null;
  return { from, to };
}

/** A knob's value after a drag of `dy` pixels up (a whole range over `span`), a step's whole. */
export function turned(k: Pick<KnobView, 'min' | 'max' | 'kind'>, value: number, dy: number, span = 160): number {
  const v = Math.min(k.max, Math.max(k.min, value + (dy / span) * (k.max - k.min)));
  return k.kind === 'step' ? Math.round(v) : v;
}

/** What PatchGraph hands its nodes and cables (Svelte context, under `GRAPH`). */
export type GraphActions = {
  knob(module: string, knob: string, value: number): void;
  open(id: string): void;
  /** Open a node out to every knob, or back. */
  toggle(id: string): void;
  opened(id: string): boolean;
  disconnect(cable: string): void;
};
export const GRAPH = 'patchgraph';
