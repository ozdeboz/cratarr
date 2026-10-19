// Cable sag after VCV Rack's CableWidget (GPL-3.0-or-later, © Andrew Belt), ported to TypeScript.
// The patch drawn as a graph (decision 0056, step 3): PatchGraph's nodes and edges from a patch, its module types and
// where things were put. Picture cables follow the patch's order (each layer's source, its modules, its own loop, into
// the blend; then the loop, finish, the screen); modulation cables are the patch's cables, from a signal or a modulator
// to a knob or a modulator's port. Positions are not the patch's (0056 keeps none): the parent keeps them by graph id
// and anything without one is laid out here. A layer blended at nothing (`blend.amount` 0: the flame after "Start from
// scratch") is off: one row, its name and "+ bring back". A muted cable (`off`) is drawn apart and moves no ring.
// Typed (0057 §1): each port and wire by its type (ports.ts): picture solid, light solid amber, value dashed, trigger
// dotted, audio a double line; a band's six at once one line marked "· 6"; a connection only where `fits` lets it, a
// picture one only where its cycle passes a Feedback; a port's normal drawn faint until a cable is plugged into it. The
// control sources' faces (faces.ts) get what they draw from. Pure.

import { endOf, type RouteValue } from './curves';
import type { Cable, Knob, Module, ModuleType, Patch, Place, SignalRef } from './patch';
import { SIGNAL_LABEL, type Signal } from './routes';
import { ENGINES } from './effects';
import { cyclesClosed, FEEDBACK, fits, MUSIC, portsOf, type PatchV2, type PortRef, type PortType, type Ports } from './ports';
import { FACES, type FaceKind } from './faces';
import { freeCableId, newCable, type Cables } from './cable-actions';

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
  /** Its type. */
  type: string;
  /** Its typed input ports past its knobs (a modulator's inputs, a source's sound), and whether a cable is in each. */
  ports: PortView[];
  /** A modulator's output (its `mod` handle): its type, its channels. */
  gives: { type: PortType; channels?: number } | null;
  /** Its outputs as jacks where it has several (the Clock's): the first is the `mod` handle. */
  outs: OutView[];
  /** A control source's live face (faces.ts): its knobs as set, its lanes (an ADSR fed six channels: six). */
  face: { kind: FaceKind; knobs: Record<string, number>; lanes: number } | null;
  /** Where the picture comes in and goes out (none: no picture). */
  in: Side | null;
  out: Side | null;
  /** A loop's last module (its picture also goes back to the loop's first), and that first. */
  feedback: boolean;
  feedbackIn: boolean;
  modulator: boolean;
};
/** A typed input port on a node (not a knob): cabled, or else normalled where its type says so. */
export type PortView = { id: string; label: string; type: PortType; channels?: number; cabled: boolean; normal?: PortRef };
/** An output jack: its handle (`mod`, or `o:<port>`). */
export type OutView = { id: string; label: string; type: PortType; handle: string };
/** A signal of Music's: its type (an event's a trigger, the sound audio), a band's six at once `channels` 6. */
export type SignalData = { kind: 'signal'; label: string; ref: SignalRef; type: PortType; channels?: number };
export type MarkData = { kind: 'blend' | 'screen'; label: string; in: Side; out: Side | null };
/** A layer's box, or the loop's; `add`: where a module added from its "+" goes (the layer's id, or `loop`). */
export type BoxData = { kind: 'box'; label: string; add?: string };
/** A layer that is off, as one row: what brings it back is its id (the flame's: `flame`). */
export type OffData = { kind: 'off'; layer: string; label: string };
/** "+ Add layer" at the picture lane's end, "+ Add modulator" by the modulators. */
export type AddData = { kind: 'add'; to: 'layer' | 'modulator'; label: string };
export type NodeData = ModuleData | SignalData | MarkData | BoxData | OffData | AddData;

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
  draggable?: false;
  zIndex?: number;
};

export type GraphEdge = {
  id: string;
  source: string;
  target: string;
  sourceHandle: string;
  targetHandle: string;
  type?: 'cable' | 'smoothstep';
  /** `picture`, `light`, `feedback`, `cable`, or `normal` (a port's normal, faint); then `w-<type>`. */
  class: string;
  label?: string;
  deletable: boolean;
  selectable: boolean;
  /** Its wire's width and dash (WIRES), as CSS variables. */
  style?: string;
  data?: CableData;
};
/** A modulation cable's (or a normal's) data: its amount, its wire's type, a bundle's channels. */
export type CableData = { amount: string; wire: PortType; channels?: number; lit?: boolean; muted?: true; normal?: true };

/** Graph ids that are not a module's (a module's node is its id). */
export const BLEND = '@blend';
export const SCREEN = '@screen';
export const LOOP = '@loop';
export const layerBox = (layer: string) => `layer:${layer}`;
export const signalKey = (r: SignalRef) => `signal:${r.signal}|${r.band ?? '-'}|${r.reference ?? 'track'}|${r.speed ?? 'fast'}`;
/** A knob's (or a port's) handle on its module's node. */
export const knobHandle = (knob: string) => `k:${knob}`;
export const MOD_OUT = 'mod';
export const ADD_LAYER = '@add-layer';
export const ADD_MODULATOR = '@add-modulator';
/** An output jack's handle past the first (which is `mod`). */
export const outHandle = (port: string) => `o:${port}`;

/** Each type's wire (0057 §1): picture solid; light solid and thicker (amber, by its class); value dashed; trigger
 *  dotted (round caps on near-nothing dashes); audio a double line (a wide stroke with its middle drawn out). */
export const WIRES: Readonly<Record<PortType, { dash: string | null; width: number; double: boolean }>> = {
  picture: { dash: null, width: 2, double: false },
  light: { dash: null, width: 3, double: false },
  value: { dash: '6 5', width: 1.5, double: false },
  trigger: { dash: '0.1 4.5', width: 2.2, double: false },
  audio: { dash: null, width: 4.5, double: true },
};
/** A muted cable's dash, whatever its type. */
export const MUTED_DASH = '2 8';
/** A wire's look as CSS variables for its path (`--wire-w`, `--wire-dash`). */
export function wireStyle(type: PortType, muted = false): string {
  const w = WIRES[type];
  return `--wire-w: ${w.width}px; --wire-dash: ${muted ? MUTED_DASH : (w.dash ?? 'none')}`;
}
/** What a bundle (a band's six at once) shows on its line, and a cable's label with its amount. */
export const bundleLabel = (channels: number | undefined) => (channels && channels > 1 ? `· ${channels}` : '');
export const cableLabel = (d: Pick<CableData, 'amount' | 'channels'>, lit: boolean) => [lit ? d.amount : '', bundleLabel(d.channels)].filter(Boolean).join(' ');

/** What a cable comes from as a port: a signal Music gives (its declared output: a trigger for an event, the sound
 *  audio; a band's signal without a band its six at once) or a modulator's output (its first). */
export function sourcePort(from: Cable['from'], typeOfModulator: (id: string) => (ModuleType & { ports?: Ports }) | undefined): { type: PortType; channels?: number } {
  if ('modulator' in from) {
    const t = typeOfModulator(from.modulator);
    const o = t ? portsOf(t).out[0] : undefined;
    return o ? { type: o.type, ...(o.channels ? { channels: o.channels } : {}) } : { type: 'value' };
  }
  const o = MUSIC.ports.out.find((x) => x.id === from.signal);
  if (!o) return { type: 'value' };
  return { type: o.type, ...(o.channels && from.band === undefined ? { channels: o.channels } : {}) };
}

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
  const parts = [SIGNAL_LABEL[r.signal as Signal] ?? MUSIC.ports.out.find((o) => o.id === r.signal)?.label ?? r.signal];
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

/** A face's drawing box: across a modulator's node; an ADSR's lane each channel; the Clock's lights are its jacks. */
export const FACE_W = 130;
export const LANE_H = 11;
export function faceHeight(f: ModuleData['face']): number {
  if (!f || f.kind === 'clock') return 0;
  return f.kind === 'adsr' && f.lanes > 1 ? f.lanes * LANE_H : 26;
}

/** A node's size before it is measured, from what it shows. */
export function sizeOf(d: NodeData): Size {
  if (d.kind === 'off') return { width: 200, height: 36 };
  if (d.kind === 'add') return { width: 130, height: 32 };
  if (d.kind !== 'module') return d.kind === 'box' ? { width: 0, height: 0 } : { width: d.kind === 'signal' ? 150 : d.kind === 'blend' ? 84 : 120, height: 36 };
  const shown = d.knobs.length + (d.hidden ? 1 : 0);
  const rows = Math.ceil(shown / KNOBS_ACROSS);
  return {
    width: Math.max(d.modulator ? 150 : 140, 20 + Math.min(shown, KNOBS_ACROSS) * KNOB_W),
    height: 38 + rows * 50 + d.ports.length * 20 + (d.outs.length ? 30 : 0) + (faceHeight(d.face) ? faceHeight(d.face) + 8 : 0),
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
  /** Whether layers, modules and modulators can be added here: the "+" nodes and the boxes' "+". */
  adds?: boolean;
  /** Which boxes take a module from their "+" (a layer's id, or `loop`); every one when not given. */
  addTo?: (where: string) => boolean;
};

/** Whether a layer is off: blended at nothing, so none of it is seen. */
export const layerOff = (l: Pick<Patch['layers'][number], 'blend'>) => l.blend.amount <= 0;

/** The patch as Svelte Flow's nodes and edges. The picture runs left to right: each layer a row in its box (its own
 *  loop at the row's end), the layers stacked, then the blend, the loop, finish and the screen in one row; the frame
 *  under the screen. Signals and modulators sit under the knobs they move, so their cables run short and mostly up. */
export function graphOf({ patch, types, positions = {}, signals = [], open = new Set(), sizes = {}, adds = false, addTo = () => true }: GraphInput): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const into = new Map<string, Cable[]>();
  const muted = (c: Cable) => !!(c as Cable & { off?: true }).off;
  for (const c of patch.cables) {
    const key = `${c.to.module}.${c.to.knob}`;
    (into.get(key) ?? into.set(key, []).get(key)!).push(c);
  }
  const size = (n: GraphNode) => sizes[n.id] ?? sizeOf(n.data);
  const put = (id: string, auto: XY) => positions[id] ?? auto;

  const modulatorType = (id: string) => {
    const m = patch.modulators.find((x) => x.id === id);
    return m ? (typeOf(m, types) as ModuleType & { ports?: Ports }) : undefined;
  };
  /** A cable's source port, its channels as many as it carries: a polyphonic module's as many as come into it (VCV
   *  Rack's rule: an ADSR fed one gate gives one envelope, fed the six bands six). */
  const portOf = (from: Cable['from'], seen = new Set<string>()): { type: PortType; channels?: number } => {
    const p = sourcePort(from, modulatorType);
    if (!('modulator' in from) || !p.channels || seen.has(from.modulator)) return p;
    seen.add(from.modulator);
    const t = modulatorType(from.modulator);
    const ins = t ? portsOf(t).in.filter((x) => x.channels) : [];
    const n = Math.max(1, ...ins.map((x) => lanesInto(from.modulator, x.id, seen)));
    return n > 1 ? { type: p.type, channels: n } : { type: p.type };
  };
  /** The most channels a cable into a port brings (a band's six at once: 6). */
  const lanesInto = (module: string, port: string, seen?: Set<string>): number =>
    Math.max(1, ...(into.get(`${module}.${port}`) ?? []).filter((c) => !muted(c)).map((c) => portOf(c.from, seen).channels ?? 1));
  /** What each module's picture output carries (a picture, or the flame's light), by its id. */
  const gives = new Map<string, PortType>();

  type Flow = { in: Side | null; out: Side | null; feedback?: boolean; feedbackIn?: boolean };
  const moduleData = (m: Module, flow: Flow): ModuleData => {
    const t = typeOf(m, types) as ModuleType & { ports?: Ports };
    const declared = portsOf(t);
    const own = new Set(t.knobs.map((k) => k.id));
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
      return { id: k.id, label: k.label, value, min: k.min, max: k.max, kind: k.kind, at, ring: ringOf(at, (into.get(`${m.id}.${k.id}`) ?? []).filter((c) => !muted(c))) };
    });
    gives.set(m.id, declared.out.find((p) => p.type === 'picture' || p.type === 'light')?.type ?? 'picture');
    const outs = modulator ? declared.out : [];
    const kind = FACES[m.type];
    return {
      kind: 'module',
      module: m.id,
      type: m.type,
      label: labelOf(t),
      place: t.place,
      knobs,
      hidden: all.length - shown.length,
      ports: declared.in
        .filter((p) => !own.has(p.id) && p.type !== 'picture' && p.type !== 'light')
        .map((p) => ({ id: p.id, label: p.label, type: p.type, ...(p.channels ? { channels: p.channels } : {}), cabled: cabled(p.id), ...(p.normal ? { normal: p.normal } : {}) })),
      gives: outs[0] ? (modulator ? portOf({ modulator: m.id }) : { type: outs[0].type }) : null,
      outs: outs.length > 1 ? outs.map((o, i) => ({ id: o.id, label: o.label, type: o.type, handle: i ? outHandle(o.id) : MOD_OUT })) : [],
      face: kind ? { kind, knobs: Object.fromEntries(t.knobs.map((k) => [k.id, m.knobs[k.id] ?? k.default])), lanes: kind === 'adsr' ? lanesInto(m.id, 'gate') : 1 } : null,
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
  const picture = (a: string, b: string, label?: string) => {
    const wire = gives.get(a) ?? 'picture';
    edges.push({ id: `pic:${a}>${b}`, source: a, target: b, sourceHandle: 'out', targetHandle: 'in', class: wire, style: wireStyle(wire), deletable: false, selectable: false, ...(label ? { label } : {}) });
  };

  /** A box around a row of modules, those from `loopAt` on a loop (its last back to its first). */
  const chain = (boxId: string, label: string, row: Module[], loopAt: number, firstIn: Side | null, origin: XY, add?: string) => {
    const data: BoxData = { kind: 'box', label, ...(adds && add && addTo(add) ? { add } : {}) };
    const box: GraphNode = { id: boxId, type: 'box', position: put(boxId, origin), data, deletable: false, selectable: false, connectable: false, zIndex: -1 };
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
      edges.push({ id: `fb:${row.at(-1)!.id}>${row[loopAt].id}`, source: row.at(-1)!.id, target: row[loopAt].id, sourceHandle: 'fb', targetHandle: 'fbin', type: 'smoothstep', class: 'feedback', style: wireStyle(gives.get(row.at(-1)!.id) ?? 'picture'), label: 'feedback', deletable: false, selectable: false });
    box.width = Math.max(120, ...inside.map((n) => n.position.x + size(n).width)) + PAD;
    box.height = Math.max(60, ...inside.map((n) => n.position.y + size(n).height)) + PAD + (loops ? 24 : 0);
    return { box, last: row.at(-1)?.id ?? null };
  };

  // The layers, one box each, stacked.
  let y = 0;
  let right = 0;
  const toBlend: { id: string; label?: string }[] = [];
  for (const l of patch.layers) {
    if (layerOff(l)) {
      const n = node(layerBox(l.id), { kind: 'off', layer: l.id, label: typeOf(l.source, types).label }, { x: 0, y });
      Object.assign(n, { selectable: false, connectable: false });
      y = n.position.y + size(n).height + 40;
      right = Math.max(right, n.position.x + size(n).width);
      continue;
    }
    const row = [l.source, ...l.modules, ...l.loop];
    const { box, last } = chain(layerBox(l.id), typeOf(l.source, types).label, row, row.length - l.loop.length, null, { x: 0, y }, l.id);
    y = box.position.y + box.height! + 40;
    right = Math.max(right, box.position.x + box.width!);
    const b = l.blend;
    if (last) toBlend.push({ id: last, label: b.mode === 'add' && b.amount === 1 ? undefined : `${b.mode} ${Math.round(b.amount * 100)}%` });
  }
  if (adds) {
    const n = node(ADD_LAYER, { kind: 'add', to: 'layer', label: '+ Add layer' }, { x: 0, y });
    Object.assign(n, { selectable: false, connectable: false, draggable: false });
    y = n.position.y + size(n).height + 40;
  }
  let bottom = Math.max(0, y - 40);

  // The blend, the picture's loop, finish and the screen, in a row level with the layers' middle.
  const mid = Math.max(0, bottom / 2 - 18);
  const blend = node(BLEND, { kind: 'blend', label: 'Blend', in: 'left', out: 'right' }, { x: right + GAP_X, y: mid });
  for (const f of toBlend) picture(f.id, BLEND, f.label);
  let from = BLEND;
  let x = blend.position.x + size(blend).width + GAP_X;
  if (patch.loop.length) {
    const { box, last } = chain(LOOP, 'Loop', patch.loop, 0, 'left', { x, y: Math.max(0, mid - BOX_TOP - 4) }, 'loop');
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
  const signalSource = (r: SignalRef) => {
    const id = signalKey(r);
    if (seen.has(id)) return id;
    seen.add(id);
    sources.push({ id, data: { kind: 'signal', label: signalLabel(r), ref: r, ...portOf(r) }, want: Infinity });
    return id;
  };
  for (const r of [...patch.cables.flatMap((c) => ('modulator' in c.from ? [] : [c.from])), ...signals]) signalSource(r);
  for (const m of patch.modulators) sources.push({ id: m.id, data: moduleData(m, { in: null, out: null }), want: Infinity });

  // Each port's normal while no cable is plugged into it (0057; Eurorack's), drawn faint: from Music, its signal; from
  // a module drawn (by its id, or the one of that type), its output's jack.
  type Normal = { from: string; handle: string; to: string; port: string; wire: PortType };
  const normals: Normal[] = [];
  const drawn = [...nodes.map((n) => n.data), ...sources.map((x) => x.data)].filter((d): d is ModuleData => d.kind === 'module');
  for (const d of drawn)
    for (const p of d.ports) {
      if (!p.normal || p.cabled) continue;
      const n = p.normal;
      if (n.module === 'music') {
        const from = signalSource({ signal: n.port, ...(n.channel !== undefined ? { band: n.channel } : {}) });
        normals.push({ from, handle: MOD_OUT, to: d.module, port: p.id, wire: p.type });
        continue;
      }
      const src = drawn.find((x) => x.module === n.module) ?? drawn.find((x) => x.type === n.module);
      const handle = src?.outs.find((o) => o.id === n.port)?.handle ?? (src?.gives && !src.outs.length ? MOD_OUT : null);
      if (src && handle && src.module !== d.module) normals.push({ from: src.module, handle, to: d.module, port: p.id, wire: p.type });
    }

  // What each source reaches (its cables' knobs and ports, its normals'), to put it under them.
  const links = new Map<string, { module: string; key: string }[]>();
  const link = (from: string, module: string, port: string) => (links.get(from) ?? links.set(from, []).get(from)!).push({ module, key: `${module}.${port}` });
  for (const c of patch.cables) link('modulator' in c.from ? c.from.modulator : signalKey(c.from), c.to.module, c.to.knob);
  for (const n of normals) link(n.from, n.to, n.port);
  for (const x of sources) {
    const xs = (links.get(x.id) ?? []).map((l) => knobX.get(l.key)).filter((v): v is number => v !== undefined);
    x.want = xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : Infinity;
  }
  type Row = { cursor: number; items: { s: Source; x: number; size: Size }[] };
  const rows: Row[] = [];
  const placedAt = new Map<string, number>();
  // What only feeds a modulator's port goes in once that modulator is placed, just left of it.
  const later = sources.filter((x) => !Number.isFinite(x.want) && links.has(x.id));
  const first = sources.filter((x) => !later.includes(x)).sort((a, b) => a.want - b.want);
  const toPort = (x: Source) => {
    const xs = (links.get(x.id) ?? []).map((l) => placedAt.get(l.module)).filter((v): v is number => v !== undefined);
    return xs.length ? Math.min(...xs) - (sizes[x.id] ?? sizeOf(x.data)).width / 2 - 24 : Infinity;
  };
  for (const x of [...first, ...later]) {
    if (later.includes(x)) x.want = toPort(x);
    const sz = sizes[x.id] ?? sizeOf(x.data);
    const desired = Number.isFinite(x.want) ? x.want - sz.width / 2 : Math.max(0, ...rows.map((r) => r.cursor));
    let row = rows.find((r) => r.cursor <= desired + SLACK);
    if (!row && rows.length < SOURCE_ROWS) rows.push((row = { cursor: -Infinity, items: [] }));
    row ??= rows.reduce((a, b) => (b.cursor < a.cursor ? b : a));
    const at = Math.max(desired, row.cursor);
    row.items.push({ s: x, x: at, size: sz });
    placedAt.set(x.id, at);
    row.cursor = at + sz.width + 16;
  }
  let sy = bottom + 120;
  for (const r of rows) {
    for (const it of r.items) {
      const n = node(it.s.id, it.s.data, { x: Math.round(it.x), y: sy });
      // Music's sound is no cable's source yet: only its normal.
      if (it.s.data.kind === 'signal' && it.s.data.type === 'audio') n.connectable = false;
    }
    sy += Math.max(...r.items.map((it) => it.size.height)) + 28;
  }
  if (adds) {
    const first = patch.modulators.map((m) => placedAt.get(m.id)).filter((v): v is number => v !== undefined);
    const n = node(ADD_MODULATOR, { kind: 'add', to: 'modulator', label: '+ Add modulator' }, { x: Math.round(first.length ? Math.min(...first) : 0), y: sy });
    Object.assign(n, { selectable: false, connectable: false, draggable: false });
  }

  // The modulation cables, where both ends are drawn, each its source's wire (a trigger into a knob stays a trigger's).
  const ids = new Set(nodes.map((n) => n.id));
  const modules = new Map(nodes.filter((n) => n.data.kind === 'module').map((n) => [n.id, n.data as ModuleData]));
  for (const c of patch.cables) {
    const source = 'modulator' in c.from ? c.from.modulator : signalKey(c.from);
    const target = modules.get(c.to.module);
    if (!ids.has(source) || !target) continue;
    if (!target.knobs.some((k) => k.id === c.to.knob) && !target.ports.some((p) => p.id === c.to.knob)) continue;
    const from = portOf(c.from);
    const data: CableData = { amount: amountLabel(c.value, c.mode), wire: from.type, ...(from.channels ? { channels: from.channels } : {}), ...(muted(c) ? { muted: true as const } : {}) };
    edges.push({ id: c.id, source, target: c.to.module, sourceHandle: MOD_OUT, targetHandle: knobHandle(c.to.knob), type: 'cable', class: 'cable', style: wireStyle(from.type, muted(c)), deletable: true, selectable: true, data });
  }
  for (const n of normals) {
    if (!ids.has(n.from) || !ids.has(n.to)) continue;
    const data: CableData = { amount: '', wire: n.wire, normal: true };
    edges.push({ id: `normal:${n.to}.${n.port}`, source: n.from, target: n.to, sourceHandle: n.handle, targetHandle: knobHandle(n.port), type: 'cable', class: `normal w-${n.wire}`, style: wireStyle(n.wire), deletable: false, selectable: false, data });
  }
  return { nodes, edges };
}

/** What a connection drawn in the graph cables, where it is one: from a signal's or a modulator's output to a knob or
 *  a port of another module (not one already cabled from there), of a type that fits it (ports.ts `fits`: the same, or
 *  a trigger into a value). */
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
  const given = src.kind === 'signal' ? src.type : src.kind === 'module' ? (src.gives?.type ?? 'value') : 'value';
  const taken = dst.ports.find((p) => p.id === to.knob)?.type ?? 'value';
  if (given === 'audio' || !fits(given, taken)) return null;
  const same = (f: Cable['from']) => ('modulator' in f && 'modulator' in from ? f.modulator === from.modulator : !('modulator' in f) && !('modulator' in from) && signalKey(f) === signalKey(from));
  if (cables.some((x) => x.to.module === to.module && x.to.knob === to.knob && same(x.from))) return null;
  return { from, to };
}

/** Whether a picture connection may be drawn (out of a module's picture into another's): only where every picture cycle
 *  it closes passes through a Feedback (0057 §4; ports.ts `cyclesClosed`), the loops' own feedback being one. */
export function pictureFits(
  c: { source: string; target: string; sourceHandle?: string | null; targetHandle?: string | null },
  nodes: { id: string; data: NodeData }[],
  edges: { source: string; target: string; class?: unknown }[],
): boolean {
  if (c.sourceHandle !== 'out' || c.targetHandle !== 'in' || c.source === c.target) return false;
  const typeOf = (id: string) => {
    const d = nodes.find((n) => n.id === id)?.data;
    return d?.kind === 'module' ? d.type : id;
  };
  const ends = [...edges.filter((e) => /^(picture|light)\b/.test(String(e.class ?? ''))), c];
  const ids = [...new Set(ends.flatMap((e) => [e.source, e.target]))];
  const p: PatchV2 = {
    v: 2,
    modules: ids.map((id) => ({ id, type: typeOf(id), knobs: {} })),
    cables: ends.map((e, i) => ({ id: `p${i}`, from: { module: e.source, port: 'picture' }, to: { module: e.target, port: 'picture' }, value: 1, mode: 'level' })),
    macros: [],
    pins: [],
  };
  const PICTURE: ModuleType & { ports: Ports } = { type: 'picture', label: '', place: 'module', knobs: [], ports: { in: [], out: [{ id: 'picture', label: '', type: 'picture' }] } };
  return cyclesClosed(p, (type) => (type === FEEDBACK.type ? FEEDBACK : PICTURE));
}

/** A knob's value after a drag of `dy` pixels up (a whole range over `span`), a step's whole. */
export function turned(k: Pick<KnobView, 'min' | 'max' | 'kind'>, value: number, dy: number, span = 160): number {
  const v = Math.min(k.max, Math.max(k.min, value + (dy / span) * (k.max - k.min)));
  return k.kind === 'step' ? Math.round(v) : v;
}

/** Which end of a cable a press is nearer: `from` (its signal's or modulator's output) or `to` (its knob or port). */
export function nearestEnd(at: XY, from: XY, to: XY): 'from' | 'to' {
  return Math.hypot(at.x - from.x, at.y - from.y) < Math.hypot(at.x - to.x, at.y - to.y) ? 'from' : 'to';
}

/** A cable's end moved (a reconnection in the graph: the cable as it was, the connection it would be now): which end
 *  moved and where to, checked as a cable drawn there would be (`cableOf`, the cable itself left out). `to`: the knob or
 *  port it goes into now; `from`: the signal or modulator it comes from. Null where it cannot go, or nothing moved. */
export function rerouted(
  cable: Cable,
  now: { source: string; target: string; sourceHandle?: string | null; targetHandle?: string | null },
  nodes: { id: string; data: NodeData }[],
  cables: Cable[],
): { end: 'from'; next: Cable['from'] } | { end: 'to'; next: Cable['to'] } | null {
  const made = cableOf(now, nodes, cables.filter((c) => c.id !== cable.id));
  if (!made) return null;
  const source = 'modulator' in cable.from ? cable.from.modulator : signalKey(cable.from);
  if (now.source !== source) return now.target === cable.to.module && now.targetHandle === knobHandle(cable.to.knob) ? { end: 'from', next: made.from } : null;
  if (made.to.module === cable.to.module && made.to.knob === cable.to.knob) return null;
  return { end: 'to', next: made.to };
}

// ---- Cable sag after VCV Rack's CableWidget (GPL-3.0-or-later, © Andrew Belt), ported to TypeScript -----------------
// Rack's src/app/CableWidget.cpp (VCVRack/Rack v2, 061ccf63), getSlumpPos and drawLayer: a cable is a quadratic curve
// from its output to its input through the point midway between them, lowered as the cable grows longer:
// `(1 - tension) * (150 + dist)`. Rack's tension setting is 0..1; here it is fixed for a slight sag on the graph's scale.
// Its opacity is drawn as `opacity ** 1.5`, as Rack's nvgAlpha(pow(opacity, 1.5)).

/** How taut a modulation cable hangs (Rack's cableTension: 1 straight, 0 its fullest sag). */
export const CABLE_TENSION = 0.9;

/** Where a cable from `a` to `b` is pulled down to: their midpoint, lower the longer the cable (Rack's getSlumpPos). */
export function slumpOf(a: XY, b: XY, tension = CABLE_TENSION): XY {
  const dist = Math.hypot(a.x - b.x, a.y - b.y);
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 + (1 - tension) * (150 + dist) };
}

/** A sagging cable's path (SVG, one quadratic curve through its slump) and its middle (where its amount shows). */
export function saggingPath(a: XY, b: XY, tension = CABLE_TENSION): [path: string, mid: XY] {
  const s = slumpOf(a, b, tension);
  const f = (v: number) => Math.round(v * 100) / 100;
  // The curve at its middle (t = 0.5): a quarter of each end and half the slump.
  const mid = { x: 0.25 * a.x + 0.5 * s.x + 0.25 * b.x, y: 0.25 * a.y + 0.5 * s.y + 0.25 * b.y };
  return [`M${f(a.x)},${f(a.y)} Q${f(s.x)},${f(s.y)} ${f(b.x)},${f(b.y)}`, mid];
}

/** The cables' opacity steps (the graph's corner button cycles them), and as drawn (Rack's `opacity ** 1.5`). */
export const CABLE_OPACITIES = [1, 0.5, 0.2] as const;
export const drawnOpacity = (o: number) => Math.pow(Math.min(1, Math.max(0, o)), 1.5);
export const nextOpacity = (o: number) => CABLE_OPACITIES[(Math.max(0, CABLE_OPACITIES.indexOf(o as (typeof CABLE_OPACITIES)[number])) + 1) % CABLE_OPACITIES.length];

// ---- "New ▸ LFO / ADSR" from a knob (0057 §4b) ----------------------------------------------------------------------

/** What a drag from a knob into the empty asks for (PatchGraph `onpick`): a source for that knob. */
export type PickRequest = { from: 'knob'; module: string; knob: string };

/** The Clock's beat as a cable takes it today (Music's beat: what the Clock's Beat output is until it has its own). */
export const CLOCK_BEAT: SignalRef = { signal: 'beat' };

/** A new modulator picked for a knob, in one change: the module, its cable into the knob, and the module synced to the
 *  Clock: each trigger input it declares that nothing is normalled into (an ADSR's gate) cabled from the beat; one
 *  normalled from the Clock already is, and is left so. `module` is the new one (its id free, its knobs set). */
export function withModulator(c: Cables, module: Module, type: ModuleType & { ports?: Ports }, to: Cable['to'], kind: Knob['kind']): { cables: Cables; cable: Cable } {
  let cables: Cables = { ...c, extra: { ...c.extra, modulators: [...c.extra.modulators, module] } };
  const own = new Set(type.knobs.map((k) => k.id));
  for (const p of portsOf(type).in) {
    if (own.has(p.id) || p.type !== 'trigger' || p.normal) continue;
    const sync: Cable = { id: freeCableId(cables), from: { ...CLOCK_BEAT }, to: { module: module.id, knob: p.id }, value: 1, mode: 'swing' };
    cables = { ...cables, extra: { ...cables.extra, cables: [...cables.extra.cables, sync] } };
  }
  const cable = newCable({ modulator: module.id }, to, kind, freeCableId(cables));
  return { cables: { ...cables, extra: { ...cables.extra, cables: [...cables.extra.cables, cable] } }, cable };
}

/** What PatchGraph hands its nodes and cables (Svelte context, under `GRAPH`). */
export type GraphActions = {
  knob(module: string, knob: string, value: number): void;
  open(id: string): void;
  /** Open a node out to every knob, or back. */
  toggle(id: string): void;
  opened(id: string): boolean;
  /** A knob tapped (not turned): what moves it. */
  openKnob(module: string, knob: string): void;
  /** Whether a node can be removed, and its long press (its menu: Delete). */
  removable(id: string): boolean;
  menu(id: string): void;
  /** A "+" tapped: the types to add there (a layer, a modulator, or a module into a box: its `add`). */
  adding(to: 'layer' | 'modulator' | { box: string }): void;
  /** A layer that is off, brought back. */
  bringBack(layer: string): void;
  /** A cable tapped (its amount, too): its configuration; its long press at a point (the window's), its menu. */
  openCable(id: string): void;
  cableMenu(id: string, at: XY): void;
  /** The cable being rerouted, and where its long press was (its nearer end is the one that moves). */
  rerouting(): { id: string; at: XY } | null;
  /** The live faces' frame (faces.ts), where the parent feeds one. */
  live(): import('./faces').LiveFrame | null;
};
export const GRAPH = 'patchgraph';
