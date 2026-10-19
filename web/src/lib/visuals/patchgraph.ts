// The patch drawn as a graph (decision 0056, step 3): PatchGraph's nodes and edges from a patch, its module types and
// where things were put. Picture cables follow the patch's order (each layer's source, its modules, its own loop, into
// the blend; then the loop, finish, the screen); modulation cables are the patch's cables, from a signal or a modulator
// to a knob or a modulator's port. Positions are not the patch's (0056 keeps none): the parent keeps them by graph id
// and anything without one is laid out here. Pure.

import { endOf, type RouteValue } from './curves';
import type { Cable, Knob, Module, ModuleType, Patch, Place, SignalRef } from './patch';
import { SIGNAL_LABEL, type Signal } from './routes';
import { ENGINES } from './effects';

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
  /** A loop's last module (its picture also goes back to the loop's first), and that first. */
  feedback: boolean;
  feedbackIn: boolean;
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
  data?: { amount: string; lit?: boolean };
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
const GAP_X = 56;
const PAD = 16;
const BOX_TOP = 30;
/** Signals and modulators: at most this many rows; one goes to another row rather than this far right of its place. */
const SOURCE_ROWS = 4;
const SLACK = 60;

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

/** A signal's name as a node shows it: its label, its band, slow, its reference where not this track. */
export function signalLabel(r: SignalRef): string {
  const parts = [SIGNAL_LABEL[r.signal as Signal] ?? r.signal];
  if (r.band !== undefined) parts.push(`${r.band + 1}`);
  if (r.speed === 'slow') parts.push('slow');
  if (r.reference && r.reference !== 'track') parts.push(r.reference);
  return parts.join(' · ');
}

/** A module's name as the panel gives it: an engine module by its engine (Kaleidoscope, EYEDO, Hydra), not its slot. */
export function labelOf(t: Pick<ModuleType, 'type' | 'label'>): string {
  const key = t.type.replace(/-engine$/, '');
  return (t.type.endsWith('-engine') && ENGINES.find((e) => e.key === key)?.label) || t.label;
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

/** The patch as Svelte Flow's nodes and edges. The picture runs left to right: each layer a row in its box (its own
 *  loop at the row's end), the layers stacked, then the blend, the loop, finish and the screen in one row; the frame
 *  under the screen. Signals and modulators sit under the knobs they move, so their cables run short and mostly up. */
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

  type Flow = { in: Side | null; out: Side | null; feedback?: boolean; feedbackIn?: boolean };
  const moduleData = (m: Module, flow: Flow): ModuleData => {
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
      label: labelOf(t),
      place: t.place,
      knobs,
      hidden: all.length - shown.length,
      ports: (t.inputs ?? []).map((p) => ({ id: p, cabled: cabled(p) })),
      in: flow.in,
      out: flow.out,
      feedback: !!flow.feedback,
      feedbackIn: !!flow.feedbackIn,
      modulator,
    };
  };

  const node = (id: string, data: NodeData, auto: XY, parent?: string): GraphNode => {
    const n: GraphNode = { id, type: 'patch', position: put(id, auto), data, deletable: false };
    if (parent) {
      Object.assign(n, { parentId: parent, extent: 'parent', expandParent: true });
      n.position = { x: Math.max(4, n.position.x), y: Math.max(BOX_TOP - 8, n.position.y) };
    }
    if (data.kind !== 'module') n.connectable = data.kind === 'signal';
    nodes.push(n);
    return n;
  };
  const picture = (a: string, b: string, label?: string) =>
    edges.push({ id: `pic:${a}>${b}`, source: a, target: b, sourceHandle: 'out', targetHandle: 'in', class: 'picture', deletable: false, selectable: false, ...(label ? { label } : {}) });

  /** A box around a row of modules, those from `loopAt` on a loop (its last back to its first). */
  const chain = (boxId: string, label: string, row: Module[], loopAt: number, firstIn: Side | null, origin: XY) => {
    const box: GraphNode = { id: boxId, type: 'box', position: put(boxId, origin), data: { kind: 'box', label }, deletable: false, selectable: false, connectable: false, zIndex: -1 };
    nodes.push(box);
    const inside: GraphNode[] = [];
    const loops = row.length - loopAt > 1;
    let x = PAD;
    row.forEach((m, i) => {
      const flow: Flow = { in: i === 0 ? firstIn : 'left', out: 'right', feedback: loops && i === row.length - 1, feedbackIn: loops && i === loopAt };
      const n = node(m.id, moduleData(m, flow), { x, y: BOX_TOP }, boxId);
      inside.push(n);
      x += size(n).width + GAP_X;
    });
    for (let i = 1; i < row.length; i++) picture(row[i - 1].id, row[i].id);
    if (loops)
      edges.push({ id: `fb:${row.at(-1)!.id}>${row[loopAt].id}`, source: row.at(-1)!.id, target: row[loopAt].id, sourceHandle: 'fb', targetHandle: 'fbin', type: 'smoothstep', class: 'feedback', label: 'feedback', deletable: false, selectable: false });
    box.width = Math.max(120, ...inside.map((n) => n.position.x + size(n).width)) + PAD;
    box.height = Math.max(60, ...inside.map((n) => n.position.y + size(n).height)) + PAD + (loops ? 24 : 0);
    return { box, last: row.at(-1)?.id ?? null };
  };

  // The layers, one box each, stacked.
  let y = 0;
  let right = 0;
  const toBlend: { id: string; label?: string }[] = [];
  for (const l of patch.layers) {
    const row = [l.source, ...l.modules, ...l.loop];
    const { box, last } = chain(layerBox(l.id), typeOf(l.source, types).label, row, row.length - l.loop.length, null, { x: 0, y });
    y = box.position.y + box.height! + 40;
    right = Math.max(right, box.position.x + box.width!);
    const b = l.blend;
    if (last) toBlend.push({ id: last, label: b.mode === 'add' && b.amount === 1 ? undefined : `${b.mode} ${Math.round(b.amount * 100)}%` });
  }
  let bottom = Math.max(0, y - 40);

  // The blend, the picture's loop, finish and the screen, in a row level with the layers' middle.
  const mid = Math.max(0, bottom / 2 - 18);
  const blend = node(BLEND, { kind: 'blend', label: 'Blend', in: 'left', out: 'right' }, { x: right + GAP_X, y: mid });
  for (const f of toBlend) picture(f.id, BLEND, f.label);
  let from = BLEND;
  let x = blend.position.x + size(blend).width + GAP_X;
  if (patch.loop.length) {
    const { box, last } = chain(LOOP, 'Loop', patch.loop, 0, 'left', { x, y: Math.max(0, mid - BOX_TOP - 4) });
    picture(BLEND, patch.loop[0].id);
    from = last!;
    x = box.position.x + box.width! + GAP_X;
    bottom = Math.max(bottom, box.position.y + box.height!);
  }
  for (const m of patch.finish) {
    const n = node(m.id, moduleData(m, { in: 'left', out: 'right' }), { x, y: Math.max(0, mid - 4) });
    picture(from, m.id);
    from = m.id;
    x = n.position.x + size(n).width + GAP_X;
    bottom = Math.max(bottom, n.position.y + size(n).height);
  }
  const screen = node(SCREEN, { kind: 'screen', label: 'Screen', in: 'left', out: null }, { x, y: mid });
  picture(from, SCREEN);
  bottom = Math.max(bottom, screen.position.y + 36);

  // The whole frame: knobs, no picture; under the screen.
  const frame = node(patch.frame.id, moduleData(patch.frame, { in: null, out: null }), { x: screen.position.x, y: bottom + 48 });
  bottom = Math.max(bottom, frame.position.y + size(frame).height);

  // Where each knob and port is across, to put what cables into it under it.
  const abs = (n: GraphNode) => {
    const p = n.parentId ? nodes.find((b) => b.id === n.parentId)!.position : { x: 0, y: 0 };
    return { x: p.x + n.position.x, y: p.y + n.position.y };
  };
  const knobX = new Map<string, number>();
  for (const n of nodes) {
    if (n.data.kind !== 'module') continue;
    const at = abs(n);
    n.data.knobs.forEach((k, i) => knobX.set(`${n.id}.${k.id}`, at.x + 10 + (i % KNOBS_ACROSS) * KNOB_W + KNOB_W / 2));
    for (const p of n.data.ports) knobX.set(`${n.id}.${p.id}`, at.x);
  }

  // Signals and modulators along the bottom, each under the middle of what it moves, in as few rows as keep them near.
  type Source = { id: string; data: NodeData; want: number };
  const sources: Source[] = [];
  const seen = new Set<string>();
  const want = (mine: (c: Cable) => boolean) => {
    const xs = patch.cables.filter(mine).map((c) => knobX.get(`${c.to.module}.${c.to.knob}`)).filter((v): v is number => v !== undefined);
    return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : Infinity;
  };
  for (const r of [...patch.cables.flatMap((c) => ('modulator' in c.from ? [] : [c.from])), ...signals]) {
    const id = signalKey(r);
    if (seen.has(id)) continue;
    seen.add(id);
    sources.push({ id, data: { kind: 'signal', label: signalLabel(r), ref: r }, want: want((c) => !('modulator' in c.from) && signalKey(c.from) === id) });
  }
  for (const m of patch.modulators) sources.push({ id: m.id, data: moduleData(m, { in: null, out: null }), want: want((c) => 'modulator' in c.from && c.from.modulator === m.id) });
  type Row = { cursor: number; items: { s: Source; x: number; size: Size }[] };
  const rows: Row[] = [];
  const placedAt = new Map<string, number>();
  // What only feeds a modulator's port goes in once that modulator is placed, just left of it.
  const later = sources.filter((s) => !Number.isFinite(s.want) && patch.cables.some((c) => (('modulator' in c.from ? c.from.modulator : signalKey(c.from)) === s.id)));
  const first = sources.filter((s) => !later.includes(s)).sort((a, b) => a.want - b.want);
  const toPort = (s: Source) => {
    const xs = patch.cables
      .filter((c) => ('modulator' in c.from ? c.from.modulator : signalKey(c.from)) === s.id)
      .map((c) => placedAt.get(c.to.module))
      .filter((v): v is number => v !== undefined);
    return xs.length ? Math.min(...xs) - (sizes[s.id] ?? sizeOf(s.data)).width / 2 - 24 : Infinity;
  };
  for (const s of [...first, ...later]) {
    if (later.includes(s)) s.want = toPort(s);
    const sz = sizes[s.id] ?? sizeOf(s.data);
    const desired = Number.isFinite(s.want) ? s.want - sz.width / 2 : Math.max(0, ...rows.map((r) => r.cursor));
    let row = rows.find((r) => r.cursor <= desired + SLACK);
    if (!row && rows.length < SOURCE_ROWS) rows.push((row = { cursor: -Infinity, items: [] }));
    row ??= rows.reduce((a, b) => (b.cursor < a.cursor ? b : a));
    const at = Math.max(desired, row.cursor);
    row.items.push({ s, x: at, size: sz });
    placedAt.set(s.id, at);
    row.cursor = at + sz.width + 16;
  }
  let sy = bottom + 120;
  for (const r of rows) {
    for (const it of r.items) node(it.s.id, it.s.data, { x: Math.round(it.x), y: sy });
    sy += Math.max(...r.items.map((it) => it.size.height)) + 28;
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
