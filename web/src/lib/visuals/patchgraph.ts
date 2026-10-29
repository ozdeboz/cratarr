// Cable sag after VCV Rack's CableWidget (GPL-3.0-or-later, © Andrew Belt), ported to TypeScript.
// The v2 patch drawn as a graph (decisions 0056 step 3, 0057 §6, §7): PatchGraph's nodes and edges from a PatchV2
// (patch-store.ts's, the app's one truth), its module types and where things were put. Positions are not the patch's
// (0056 keeps none): the parent keeps them by node path and anything without one is laid out here (or by ELK,
// elk-layout.ts). Typed (0057 §1): each port and wire by its type (ports.ts): picture solid, light solid amber, value
// dashed, trigger dotted, audio a double line, points dash-dot, flow a double line in long dashes; a band's six at once
// one line marked "· 6"; a connection only where `fits` lets it, a picture one only where its cycle passes a Feedback;
// a port's normal drawn faint until a cable is plugged into it. The control sources' faces (faces.ts) get what they
// draw from. Edits come back as V2Edit (applyV2) on a copy, so the store keeps each as a step. Pure.

import { endOf, type RouteValue } from './curves';
import type { Cable, Knob, Module, ModuleType, Place, SignalRef } from './patch';
import { ENGINES } from './effects';
import { cyclesClosed, FEEDBACK, fits, isDevice, polyKnobs, portsOf, portsOfModule, type CableV2, type Device, type ExposedPort, type PatchV2, type Polymorphic, type Port, type PortRef, type PortType, type Ports } from './ports';
import { byCategory, CATEGORIES, CATEGORY_LABEL, categoryOf, type Category } from './rack';
import { signalOf } from './music-signals';
import { areaParts, faceHeight as partsHeight, faceOf, type FacePart, type Thumbs } from './face/face';
import { freeCableId, newCable, type Cables } from './cable-actions';
import { makeDevice, unmakeDevice, type MacroPick } from './device-make';
import { flatten, macroNode, mappingOf } from './v2';

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
  /** Pinned (0057: `<module>.<knob>` in its level's pins): Randomize and a new track leave it. */
  pinned?: boolean;
  /** A step's choices by name (Knob `options`). */
  options?: readonly string[];
  /** The macro that maps it (0077: one a knob; the µMAP marker): its name and its range's ends on the dial (0..1). Its
   *  value is as the macro sets it; it does not turn by hand. */
  macro?: { label: string; span: [number, number] };
  /** A polyphonic knob drawn as one (0057 §4b: a band's six): each channel's knob id, value and place in the range; the
   *  knob's own `value` and `at` their mean. */
  channels?: { id: string; value: number; at: number }[];
  /** A step knob's `<id>:next` trigger (0057 §7e), drawn as a small diamond beside it; a level knob's `<id>:gain`
   *  input (§7e), a small dot beside it; `nextOn`/`gainOn` where a cable is in it (else shown only on the node in
   *  focus, so a panel of knobs is not strewn with them). */
  next?: boolean;
  gain?: boolean;
  nextOn?: boolean;
  gainOn?: boolean;
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
  /** Its outputs past its picture's, each a labelled jack down its right edge (a modulator's first is the `mod` handle). */
  outs: OutView[];
  /** A control source's live face (faces.ts): its knobs as set, its lanes (an ADSR fed six channels: six). */
  face: { parts: FacePart[]; knobs: Record<string, number>; lanes: number } | null;
  /** Where the picture comes in and goes out (none: no picture), and the types its picture ports carry. */
  in: Side | null;
  out: Side | null;
  pinType?: PortType;
  poutType?: PortType;
  /** What a polymorphic picture output carries, from what comes into it (withCarried). */
  carried?: PortType;
  /** How far right of its left edge the node's far form may reach before the next node (flow px). */
  room?: number;
  /** How many cables come in and go out (its far form says so). */
  cables?: Count;
  /** Its category's lane (v2): ELK keeps the lanes in order by it. */
  category?: Category;
  modulator: boolean;
  /** Its ports whole, which of them the picture handles and the `mod` handle are. */
  v2: { ports: Ports; pin: string | null; pout: string | null; mod: string | null };
  /** A device (0057 §6): opened in place (drawn as a frame round its modules instead). */
  device?: { open: boolean };
  /** Pinned whole (`module:<id>`, a module or a device). */
  pinned?: boolean;
  /** A music module of one signal (music-signals.ts): which; it sits beside what it feeds (its category theirs). */
  music?: string;
};
/** A typed input port on a node (not a knob): cabled, or else normalled where its type says so. */
export type PortView = { id: string; label: string; type: PortType; channels?: number; cabled: boolean; normal?: PortRef; polymorphic?: Polymorphic };
/** An output jack: its handle (`mod`, or `o:<port>`). */
export type OutView = { id: string; label: string; type: PortType; handle: string };
/** Cables into a node and out of it. */
export type Count = { in: number; out: number };
/** A device opened in place: the frame round its modules. */
export type BoxData = { kind: 'box'; label: string; device?: true; category?: Category };
/** A category's lane's header (v2): its name, whether the whole category is pinned, the devices it is inside. */
export type LaneData = { kind: 'lane'; category: Category; label: string; pinned: boolean; scope: string[] };
export type NodeData = ModuleData | BoxData | LaneData;

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
  /** `cable` (a modulation cable, sagging), `wire` (a picture's or light's, a curve or, `data.step`, in steps). */
  type?: 'cable' | 'wire';
  /** `picture`, `light`, `cable`, or `normal` (a port's normal, faint); then `w-<type>`. */
  class: string;
  label?: string;
  deletable: boolean;
  selectable: boolean;
  /** Its wire's width and dash (WIRES), as CSS variables. */
  style?: string;
  data?: CableData;
};
/** A modulation cable's (or a normal's) data: its amount, its wire's type, a bundle's channels. */
export type CableData = { amount: string; wire: PortType; channels?: number; lit?: boolean; muted?: true; normal?: true; step?: boolean; route?: XY[]; pinned?: boolean };

/** A knob's (or a port's) handle on its module's node. */
export const knobHandle = (knob: string) => `k:${knob}`;
export const MOD_OUT = 'mod';
/** An output jack's handle past the first (which is `mod`). */
export const outHandle = (port: string) => `o:${port}`;

/** A jack's size (every socket is one, PatchNode's `.jack`), and where a cable meets it: its centre, half a jack in
 *  from the edge point Svelte Flow gives on the jack's side. */
export const JACK = 10;
export function toJack(x: number, y: number, side: Side | string | undefined): XY {
  const r = JACK / 2;
  if (side === 'left') return { x: x + r, y };
  if (side === 'right') return { x: x - r, y };
  if (side === 'top') return { x, y: y + r };
  if (side === 'bottom') return { x, y: y - r };
  return { x, y };
}
/** A route (ELK's, from its port to its port) drawn from jack centre `a` to jack centre `b`: its first and last bends slid
 *  onto the jacks' lines (ELK's port is near the jack, not on it), a square corner wherever two points are still not in
 *  line (across first, or down first in portrait), and points in line merged. */
export function routePath(a: XY, route: XY[], b: XY, portrait = false): [path: string, mid: XY] {
  const bends = route.slice(1, -1).map((p) => ({ ...p }));
  const along = (p: XY, q: XY) => (Math.abs(p.y - q.y) < 0.5 ? 'x' : Math.abs(p.x - q.x) < 0.5 ? 'y' : null);
  if (bends.length) {
    const first = along(route[0], route[1]);
    if (first === 'x') bends[0].y = a.y;
    else if (first === 'y') bends[0].x = a.x;
    const last = along(route[route.length - 2], route[route.length - 1]);
    const z = bends[bends.length - 1];
    if (last === 'x') z.y = b.y;
    else if (last === 'y') z.x = b.x;
  }
  const pts: XY[] = [a, ...bends, b];
  const square: XY[] = [pts[0]];
  for (let i = 1; i < pts.length; i++) {
    const p = square[square.length - 1];
    const q = pts[i];
    if (Math.abs(p.x - q.x) > 0.5 && Math.abs(p.y - q.y) > 0.5) square.push(portrait ? { x: p.x, y: q.y } : { x: q.x, y: p.y });
    square.push(q);
  }
  // Points in line with both neighbours, and repeats, dropped.
  const clean = square.filter((p, i) => {
    if (i === 0 || i === square.length - 1) return true;
    const o = square[i - 1];
    const q = square[i + 1];
    const same = Math.abs(p.x - o.x) < 0.5 && Math.abs(p.y - o.y) < 0.5;
    const line = (Math.abs(o.x - p.x) < 0.5 && Math.abs(p.x - q.x) < 0.5) || (Math.abs(o.y - p.y) < 0.5 && Math.abs(p.y - q.y) < 0.5);
    return !same && !line;
  });
  const f = (v: number) => Math.round(v * 10) / 10;
  const path = clean.map((p, i) => `${i ? 'L' : 'M'}${f(p.x)},${f(p.y)}`).join(' ');
  return [path, clean[Math.floor(clean.length / 2)]];
}

/** The zoom at which a node shows whole and its smallest text is 11 px on screen (knob labels: 11 px at 1): the graph
 *  opens there and no fit goes below it; under it is the overview (only by the viewer's own zoom). */
export const FULL_ZOOM = 1;
export type Pad = { top: number; right: number; bottom: number; left: number };
export type Rect = { x: number; y: number; width: number; height: number };

/** Where the graph opens (VCV Rack's way): at FULL_ZOOM, modules whole. Along each axis the patch is centred where it
 *  fits in the view (inside its padding), else its start (left, top) is at the view's start: a patch bigger than the
 *  screen is panned to, not shrunk. Svelte Flow's viewport: the flow's origin on screen and the zoom. */
export function openView(b: Rect, view: Size, pad: Pad, zoom = FULL_ZOOM, first?: XY): { x: number; y: number; zoom: number } {
  const w = view.width - pad.left - pad.right;
  const h = view.height - pad.top - pad.bottom;
  // Where the patch does not fit, it starts at its first module (`first`: where the signals start), else its corner.
  const axis = (start: number, extent: number, room: number, lead: number, from: number) => (extent * zoom <= room ? lead + (room - extent * zoom) / 2 - start * zoom : lead - from * zoom);
  return { x: axis(b.x, b.width, w, pad.left, first?.x ?? b.x), y: axis(b.y, b.height, h, pad.top, first?.y ?? b.y), zoom };
}

/** The module the signals start at: the one that only gives (cables out, none in) and gives the most (Music); where
 *  none does, the one nearest the top left corner of the modules and frames, the higher of two as near. */
export function firstModule(nodes: { position: XY; parentId?: string; data: NodeData }[], _portrait = false): XY | undefined {
  const top = nodes.filter((n) => !n.parentId && (n.data.kind === 'module' || n.data.kind === 'box'));
  if (!top.length) return undefined;
  // (Not a music module of one signal: a small one beside what it feeds, not where the rack starts.)
  const givers = top.filter((n) => n.data.kind === 'module' && !n.data.music && n.data.cables && !n.data.cables.in && n.data.cables.out > 0);
  if (givers.length) return givers.reduce((a, b) => ((b.data as ModuleData).cables!.out > (a.data as ModuleData).cables!.out ? b : a)).position;
  const corner = { x: Math.min(...top.map((n) => n.position.x)), y: Math.min(...top.map((n) => n.position.y)) };
  const far = (p: XY) => Math.hypot(p.x - corner.x, p.y - corner.y);
  let best = top[0].position;
  for (const n of top) if (far(n.position) < far(best) - 0.5 || (Math.abs(far(n.position) - far(best)) <= 0.5 && n.position.y < best.y)) best = n.position;
  return best;
}

/** Cables over the nodes (as VCV Rack draws them): every edge's z-index. */
export const EDGE_Z = 1000;

/** Each type's wire (0057 §1): picture solid; light solid and thicker (amber, by its class); value dashed; trigger
 *  dotted (round caps on near-nothing dashes); audio a double line (a wide stroke with its middle drawn out). */
export const WIRES: Readonly<Record<PortType, { dash: string | null; width: number; double: boolean }>> = {
  picture: { dash: null, width: 2, double: false },
  light: { dash: null, width: 3, double: false },
  value: { dash: '6 5', width: 1.5, double: false },
  trigger: { dash: '0.1 4.5', width: 2.2, double: false },
  audio: { dash: null, width: 4.5, double: true },
  // A chain line: dash, dot (0057 §4c).
  points: { dash: '7 3 0.1 3', width: 2, double: false },
  // A flow (0057 §4h): a double line in long dashes, a field moving.
  flow: { dash: '12 4', width: 4, double: true },
};
/** A muted cable's dash, whatever its type. */
export const MUTED_DASH = '2 8';
/** A wire's style as CSS variables for its path (`--wire-w`, `--wire-dash`). */
export function wireStyle(type: PortType, muted = false): string {
  const w = WIRES[type];
  return `--wire-w: ${w.width}px; --wire-dash: ${muted ? MUTED_DASH : (w.dash ?? 'none')}`;
}
/** What a bundle (a band's six at once) shows on its line, and a cable's label with its amount. */
export const bundleLabel = (channels: number | undefined) => (channels && channels > 1 ? `· ${channels}` : '');
export const cableLabel = (d: Pick<CableData, 'amount' | 'channels'>, lit: boolean) => [lit ? d.amount : '', bundleLabel(d.channels)].filter(Boolean).join(' ');

const KNOB_W = 44;
const KNOBS_ACROSS = 6;
/** How many knobs a module shows before it is opened out (its cabled ones always); a modulator shows its cabled ones. */
const SHOWN = 3;
const GAP_X = 56;
/** How wide a lane's rows run (portrait, v2), and the room between its modules for a far label. */
const PORTRAIT_ACROSS = 900;
const PORTRAIT_BESIDE = 150;
const PAD = 16;
const BOX_TOP = 30;

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

/** A module's name as the panel gives it: an engine module by its engine (Kaleidoscope, EYEDO, Hydra), not its slot. */
export function labelOf(t: Pick<ModuleType, 'type' | 'label'>): string {
  const key = t.type.replace(/-engine$/, '');
  return (t.type.endsWith('-engine') && ENGINES.find((e) => e.key === key)?.label) || RENAMED[t.type] || t.label;
}
/** Names the graph gives ahead of their types (0057 §7e: the course-and-swing filter is Accent; "flash" is the Screen's). */
const RENAMED: Record<string, string> = { flash: 'Accent' };
/** A module's name, as the panel and the graph both give it: its own (a device's; one its device's maker gave it,
 *  device-names.ts), else its type's, numbered where its level has more than one of that type unnamed. */
export function moduleName(m: Module, level: Pick<PatchV2, 'modules'>, types: Record<string, ModuleType>): string {
  if (m.label) return m.label;
  if (isDevice(m)) return 'Device';
  const t = types[m.type];
  const base = t ? labelOf(t) : m.type;
  const same = level.modules.filter((x) => x.type === m.type && !x.label);
  return same.length > 1 ? `${base} ${same.indexOf(m) + 1}` : base;
}

/** A cable's amount as its label shows it: a swing signed, a level as a share. */
/** A v2 cable (0057 §7e item 10) has no mode: it adds, a swing. */
export function amountLabel(value: RouteValue, mode: Cable['mode'] = 'swing'): string {
  const pc = Math.round(endOf(value) * 100);
  return mode === 'swing' && pc > 0 ? `+${pc}%` : `${pc}%`.replace('-', '−');
}

/** The stretch of a knob's range (0..1) its cables move it over: a swing's amount above (or below) where it is set, a
 *  level's share under it. Amounts are of the knob's whole range; none where nothing is cabled. */
export function ringOf(at: number, cables: (Pick<Cable, 'value'> & { mode?: Cable['mode'] })[]): [number, number] | null {
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

/** A node's face area's height (face/face.ts): its parts not drawn beside an output. */
export function faceHeight(f: ModuleData['face'], outs: readonly string[] = []): number {
  return f ? partsHeight(areaParts(f.parts, outs), f.lanes) : 0;
}

/** A step knob's trigger port (ports.ts: `<knob>:next`), and whether a port id is one. */
export const nextPort = (knob: string) => `${knob}:next`;
export const gainPort = (knob: string) => `${knob}:gain`;
/** A port drawn by its knob (its next, its gain), not in the port rows. */
const isNext = (port: string) => port.endsWith(':next') || port.endsWith(':gain');

/** A module's knobs as drawn (shared by the v1 and v2 graphs): a band's knobs (`lighting.0` … `.5`) as one polyphonic
 *  knob (0057 §4b; ports.ts polyKnobs), its channels kept as `<id>.<channel>`, opened out (`<module>.<knob>` in `open`)
 *  to one knob a channel; `room` of them shown before the node is opened out (`<module>` in `open`), the cabled ones
 *  always; each with its ring from the cables into it. */
export function knobViews(
  m: Module,
  t: ModuleType,
  o: {
    open: ReadonlySet<string>;
    room: number;
    cabled: (knob: string) => boolean;
    cablesInto: (knob: string) => (Pick<Cable, 'value'> & { mode?: Cable['mode'] })[];
    /** The macro mapping a knob of it, if one does (v2.ts mappingOf). */
    mapped?: (knob: string) => { label: string; from: number; to: number; value: number } | undefined;
  },
): { knobs: KnobView[]; hidden: number } {
  const flat = t.knobs.filter((k) => !k.device);
  const byId = new Map(flat.map((k) => [k.id, k]));
  const channelKnobs = (k: Knob): Knob[] =>
    Array.from({ length: k.channels ?? 1 }, (_, c) => byId.get(`${k.id}.${c}`) ?? { ...k, id: `${k.id}.${c}`, label: `${k.label} ${c + 1}`, channels: undefined });
  const outOf = new Set<string>();
  const all = polyKnobs({ ...t, knobs: flat }).knobs.flatMap((k) => {
    if (!k.channels || !o.open.has(`${m.id}.${k.id}`)) return [k];
    const each = channelKnobs(k);
    for (const x of each) outOf.add(x.id);
    return each;
  });
  const hasNext = (k: Knob) => k.kind === 'step' && k.randomize !== false;
  const hasGain = (k: Knob) => k.kind === 'level';
  const own = (k: Knob) => (k.channels ? channelKnobs(k).map((x) => x.id) : [k.id]);
  const ids = (k: Knob) => [...own(k), ...(hasNext(k) ? [nextPort(k.id)] : []), ...(hasGain(k) ? [gainPort(k.id)] : [])];
  const opened = o.open.has(m.id);
  let room = o.room;
  const shown = all.filter((k) => {
    // Opened out, a polyphonic knob's channels show whole.
    if (opened || outOf.has(k.id) || ids(k).some(o.cabled)) return true;
    return room-- > 0;
  });
  const atOf = (k: Knob, value: number) => (k.max > k.min ? clamp01((value - k.min) / (k.max - k.min)) : 0);
  // Shown in the type's order, cabled or not.
  const knobs: KnobView[] = shown.map((k) => {
    const ring = (at: number) => ringOf(at, own(k).flatMap(o.cablesInto));
    const beside = {
      ...(hasNext(k) ? { next: true, ...(o.cabled(nextPort(k.id)) ? { nextOn: true } : {}) } : {}),
      ...(hasGain(k) ? { gain: true, ...(o.cabled(gainPort(k.id)) ? { gainOn: true } : {}) } : {}),
    };
    const named = k.options ? { options: k.options } : {};
    const macroOf = (g: { label: string; from: number; to: number } | undefined) => (g ? { macro: { label: g.label, span: [atOf(k, g.from), atOf(k, g.to)] as [number, number] } } : {});
    if (!k.channels) {
      const g = o.mapped?.(k.id);
      const value = g ? g.value : (m.knobs[k.id] ?? k.default);
      return { id: k.id, label: k.label, value, min: k.min, max: k.max, kind: k.kind, at: atOf(k, value), ring: ring(atOf(k, value)), ...named, ...beside, ...macroOf(g) };
    }
    let first: ReturnType<NonNullable<typeof o.mapped>>;
    const channels = channelKnobs(k).map((x) => {
      const g = o.mapped?.(x.id);
      first ??= g;
      const value = g ? g.value : (m.knobs[x.id] ?? m.knobs[k.id] ?? x.default);
      return { id: x.id, value, at: atOf(k, value) };
    });
    const value = channels.reduce((a, c) => a + c.value, 0) / channels.length;
    return { id: k.id, label: k.label, value, min: k.min, max: k.max, kind: k.kind, at: atOf(k, value), ring: ring(atOf(k, value)), channels, ...named, ...beside, ...macroOf(first) };
  });
  return { knobs, hidden: all.length - shown.length };
}

/** What a face reads of its module's knobs: each at its default, as set, a polyphonic one's channels too. */
export const faceKnobs = (m: Module, t: ModuleType): Record<string, number> => ({ ...Object.fromEntries(t.knobs.map((k) => [k.id, k.default])), ...m.knobs });

/** The top-level nodes' extent (flow px). */
export function extent(nodes: GraphNode[], size: (n: GraphNode) => Size): Size {
  const top = nodes.filter((n) => !n.parentId);
  const right = Math.max(0, ...top.map((n) => n.position.x + (n.width ?? size(n).width)));
  const bottom = Math.max(0, ...top.map((n) => n.position.y + (n.height ?? size(n).height)));
  const left = Math.min(0, ...top.map((n) => n.position.x));
  const up = Math.min(0, ...top.map((n) => n.position.y));
  return { width: right - left, height: bottom - up };
}
/** The zoom a fit of `e` into `view` comes to. */
export const fitZoom = (e: Size, view: Size) => Math.min(view.width / Math.max(1, e.width), view.height / Math.max(1, e.height));

/** A knob's step or gain jack with a cable on it is on (shown), wherever the cable is kept (one led in through a
 *  device's port too). */
export function withJacksOn(nodes: GraphNode[], edges: GraphEdge[]) {
  const on = new Set(edges.map((e) => `${e.target}|${e.targetHandle}`));
  for (const n of nodes) {
    if (n.data.kind !== 'module') continue;
    const d = n.data;
    if (!d.knobs.some((k) => (k.next && !k.nextOn && on.has(`${n.id}|${knobHandle(nextPort(k.id))}`)) || (k.gain && !k.gainOn && on.has(`${n.id}|${knobHandle(gainPort(k.id))}`)))) continue;
    n.data = {
      ...d,
      knobs: d.knobs.map((k) => ({
        ...k,
        ...(k.next && on.has(`${n.id}|${knobHandle(nextPort(k.id))}`) ? { nextOn: true } : {}),
        ...(k.gain && on.has(`${n.id}|${knobHandle(gainPort(k.id))}`) ? { gainOn: true } : {}),
      })),
    };
  }
}

/** Each node's cables in and out (not its normals): its far form counts them. */
export function withCounts(nodes: GraphNode[], edges: GraphEdge[]) {
  const count = new Map<string, Count>();
  const at = (id: string) => count.get(id) ?? count.set(id, { in: 0, out: 0 }).get(id)!;
  for (const e of edges) {
    if (e.data?.normal) continue;
    at(e.source).out++;
    at(e.target).in++;
  }
  for (const n of nodes) if (n.data.kind === 'module') n.data = { ...n.data, cables: count.get(n.id) ?? { in: 0, out: 0 } };
}
/** A node's cables as its far form says them: "6 in · 12 out", one side alone where the other has none. */
export function countLabel(c: Count | undefined): string {
  if (!c) return '';
  return [c.in ? `${c.in} in` : '', c.out ? `${c.out} out` : ''].filter(Boolean).join(' · ');
}


/** How far right a node's far form may reach (its `room`): to the next node beside it that its row overlaps, less a
 *  little; far from any, `ROOM_MOST`. Positions as laid out (a node in a box by its box's). */
const ROOM_MOST = 900;
export function withRoom(nodes: GraphNode[], size: (n: GraphNode) => Size) {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const abs = (n: GraphNode): XY => {
    const p = n.parentId ? byId.get(n.parentId) : undefined;
    const at = p ? abs(p) : { x: 0, y: 0 };
    return { x: at.x + n.position.x, y: at.y + n.position.y };
  };
  const boxes = nodes
    .filter((n) => n.type === 'patch' && n.data.kind !== 'box')
    .map((n) => {
      const at = abs(n);
      const sz = size(n);
      return { n, x: at.x, y: at.y, w: sz.width, h: sz.height };
    });
  // Never past the graph's own right edge (a fit would leave it off the screen).
  const right = Math.max(0, ...boxes.map((b) => b.x + b.w));
  for (const a of boxes) {
    if (a.n.data.kind !== 'module') continue;
    let room = Math.min(ROOM_MOST, right - a.x);
    for (const b of boxes) if (b !== a && b.x > a.x && b.y < a.y + a.h && b.y + b.h > a.y) room = Math.min(room, b.x - a.x - 12);
    a.n.data = { ...a.n.data, room: Math.max(a.w, room) };
  }
}

/** A node's size before it is measured, from what it shows. */
export function sizeOf(d: NodeData): Size {
  if (d.kind === 'lane') return { width: 220, height: 26 };
  if (d.kind === 'box') return { width: 0, height: 0 };
  const shown = d.knobs.length + (d.hidden ? 1 : 0);
  const rows = Math.ceil(shown / KNOBS_ACROSS);
  const io = Math.max(d.ports.length + (d.in ? 1 : 0), d.outs.length + (d.out ? 1 : 0));
  return {
    width: Math.max(d.modulator ? 150 : 140, 20 + Math.min(shown, KNOBS_ACROSS) * KNOB_W),
    height: 38 + rows * 60 + (io ? io * 20 + 6 : 0) + (faceHeight(d.face, d.outs.map((o) => o.id)) ? faceHeight(d.face, d.outs.map((o) => o.id)) + 8 : 0),
  };
}

/** A polyphonic knob drawn as one, turned to `value`: each channel moved as far, so they keep their differences
 *  (each clamped to the range). */
export function polyTurned(k: Pick<KnobView, 'value' | 'min' | 'max' | 'kind' | 'channels'>, value: number): { id: string; value: number }[] {
  const d = value - k.value;
  return (k.channels ?? []).map((c) => {
    const v = Math.min(k.max, Math.max(k.min, c.value + d));
    return { id: c.id, value: k.kind === 'step' ? Math.round(v) : v };
  });
}

// ---- Fine adjust: a knob's long press (the owner, after the player's waveform lens and iOS's scrubbing) -------------

/** How far sideways a knob's whole range is at full speed (px). */
export const FINE_SPAN = 240;
/** The speed a fine adjust moves at: full within 40 px of where the finger was held, half to 80, a quarter to 160, an
 *  eighth beyond (the further the finger has moved up from the slider, the finer). */
export function fineSpeed(up: number): number {
  const d = Math.max(0, up);
  return d <= 40 ? 1 : d <= 80 ? 0.5 : d <= 160 ? 0.25 : 0.125;
}
export const speedLabel = (speed: number) => (speed >= 1 ? '×1' : speed >= 0.5 ? '×½' : speed >= 0.25 ? '×¼' : '×⅛');
/** A fine adjust's step: `raw` (the unrounded value carried between moves) moved by `dx` px at `speed`, kept in the
 *  range; the knob's value is that, a step knob's rounded to its whole step. */
export function fineStep(k: Pick<KnobView, 'min' | 'max' | 'kind'>, raw: number, dx: number, speed: number): { raw: number; value: number } {
  const r = Math.min(k.max, Math.max(k.min, raw + (dx / FINE_SPAN) * (k.max - k.min) * speed));
  return { raw: r, value: k.kind === 'step' ? Math.round(r) : r };
}
/** A knob's value as the fine adjust shows it: a step's whole number, else as many places as tell it apart. */
export function fineText(k: Pick<KnobView, 'kind' | 'min' | 'max' | 'options'>, value: number): string {
  if (k.kind === 'step') return stepText(k, value);
  const span = Math.abs(k.max - k.min);
  return value.toFixed(span >= 100 ? 0 : span >= 10 ? 1 : span >= 1 ? 3 : 4);
}
/** A step's value as its choice's name, else its number. */
const stepText = (k: Pick<KnobView, 'min' | 'options'>, value: number) => k.options?.[Math.round(value) - k.min] ?? String(Math.round(value));
/** A knob's value in words, as the panel's rows and the graph's dial say it: a step's choice by its name, a share as a
 *  percent, a swing about nothing signed, else its number. */
export function knobText(k: Pick<KnobView, 'kind' | 'min' | 'max' | 'options'>, value: number): string {
  if (k.kind === 'step') return stepText(k, value);
  if (k.min === 0 && k.max === 1) return `${Math.round(value * 100)}%`;
  if (k.min === -1 && k.max === 1) return `${value > 0 ? '+' : ''}${Math.round(value * 100)}%`;
  return Math.abs(value) >= 10 ? value.toFixed(0) : value.toFixed(2);
}
/** A fine adjust under way: the knob (its node and id, name, range, kind, choices), its value, its speed, where it was
 *  held (the window's coordinates). */
export type Fine = { node: string; knob: string; label: string; min: number; max: number; kind: Knob['kind']; options?: readonly string[]; value: number; speed: number; at: XY };

/** A knob's value after a drag of `dy` pixels up (a whole range over `span`), a step's whole. */
export function turned(k: Pick<KnobView, 'min' | 'max' | 'kind'>, value: number, dy: number, span = 160): number {
  const v = Math.min(k.max, Math.max(k.min, value + (dy / span) * (k.max - k.min)));
  return k.kind === 'step' ? Math.round(v) : v;
}

/** Which end of a cable a press is nearer: `from` (its signal's or modulator's output) or `to` (its knob or port). */
export function nearestEnd(at: XY, from: XY, to: XY): 'from' | 'to' {
  return Math.hypot(at.x - from.x, at.y - from.y) < Math.hypot(at.x - to.x, at.y - to.y) ? 'from' : 'to';
}

// ---- Cable sag after VCV Rack's CableWidget (GPL-3.0-or-later, © Andrew Belt), ported to TypeScript -----------------
// Rack's src/app/CableWidget.cpp (VCVRack/Rack v2, 061ccf63), getSlumpPos and drawLayer: a cable is a quadratic curve
// from its output to its input through the point midway between them, lowered as the cable grows longer:
// `(1 - tension) * (150 + dist)`. Rack's tension setting is 0..1; here it is fixed for a slight sag on the graph's scale.

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


// ---- "New ▸ LFO / ADSR" from a knob (0057 §4b) ----------------------------------------------------------------------

/** What a drag from a knob into the empty asks for (PatchGraph `onpick`): a source for that knob. */
export type PickRequest = { from: 'knob'; module: string[]; knob: string };

// withModulator is Visuals.svelte's (its v1 cables) until the store is mounted there; it goes with step 6 (S5).

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

// ---- A v2 patch drawn (0057 §6, §7): modules and cables between typed ports, devices as nodes ------------------------
// The patch as the store holds it (patch-store.ts), drawn level by level: the level shown is the one `at` leads to
// (the devices entered, from the top); a device there is one node with its exposed ports and its macros as knobs,
// or, opened in place (its id in `open`), a frame round its own modules and cables, nested as deep as devices go. Node
// ids are paths from the level shown (`show/bend`). The lanes are rack.ts's: the sources down the left, the picture's
// modules in the order pictures flow, the output after them, the control below in rows. Edits come back as V2Edit
// (applyV2), so the store can keep each one as a step.

export type V2Input = {
  patch: PatchV2;
  types: Record<string, ModuleType>;
  /** The devices entered, from the top. */
  at?: readonly string[];
  /** Devices opened in place, nodes opened out to every knob, polyphonic knobs opened out (`<node>.<knob>`). */
  open?: ReadonlySet<string>;
  positions?: Record<string, XY>;
  sizes?: Record<string, Size>;
  /** A screen taller than wide: each lane's row turns sooner, so the lanes run down, as wide as fits `view` best. */
  portrait?: boolean;
  view?: Size;
};

/** The patch inside the devices `at` leads through; null where one is not there. */
export function patchAt(p: PatchV2, at: readonly string[]): PatchV2 | null {
  let cur = p;
  for (const id of at) {
    const d = cur.modules.find((m) => m.id === id);
    if (!d || !isDevice(d)) return null;
    cur = d.patch;
  }
  return cur;
}

/** The names along `at` (the breadcrumb): each device's label. */
export function crumbsOf(p: PatchV2, at: readonly string[]): string[] {
  const out: string[] = [];
  let cur: PatchV2 | null = p;
  for (const id of at) {
    const d: Module | undefined = cur?.modules.find((m) => m.id === id);
    out.push(d && isDevice(d) ? d.label : id);
    cur = d && isDevice(d) ? d.patch : null;
  }
  return out;
}


/** The knob behind a device's exposed input, where it is one (through nested devices): the knob and its value. */
function knobBehind(d: Device, as: string, types: Record<string, ModuleType>): { knob: Knob; value: number } | null {
  const e = d.inputs.find((x) => x.as === as);
  const inner = e ? d.patch.modules.find((x) => x.id === e.module) : undefined;
  if (!e || !inner) return null;
  if (isDevice(inner)) return knobBehind(inner, e.port, types);
  const t = types[inner.type];
  const k = t ? polyKnobs(t).knobs.find((x) => x.id === e.port) : undefined;
  if (!k) return null;
  if (!k.channels) return { knob: k, value: inner.knobs[k.id] ?? k.default };
  const vs = Array.from({ length: k.channels }, (_, c) => inner.knobs[`${k.id}.${c}`] ?? inner.knobs[k.id] ?? k.default);
  return { knob: { ...k, channels: undefined }, value: vs.reduce((a, b) => a + b, 0) / vs.length };
}

const PICTURE_TYPES: ReadonlySet<PortType> = new Set<PortType>(['picture', 'light']);
/** How far across a lane's row runs before it turns; a lane's header's height. */
const LANE_ACROSS = 1400;
const LANE_HEAD = 34;
/** A lane's header's node id, and the pin that holds a whole category (0057 §3b). */
export const laneId = (c: Category) => `@lane:${c}`;
export const categoryPin = (c: Category) => `category:${c}`;
/** A macro's knob on its device. */
export const macroKnob = (id: string) => `macro:${id}`;

/** A handle of a v2 node for one of its ports: its picture in or out (`in`, `out`), its `mod` output, an output jack
 *  (`o:<port>`), else an input (`k:<port>`, a knob's too). */
export function handleOf(d: ModuleData, dir: 'in' | 'out', port: string): string {
  if (dir === 'in') return d.v2?.pin === port ? 'in' : knobHandle(port);
  if (d.v2?.pout === port) return 'out';
  if (d.v2?.mod === port) return MOD_OUT;
  return outHandle(port);
}

/** The handle a v2 node has for a port, where it draws one (a cable into one channel of a polyphonic knob drawn as one:
 *  that knob's); null where it has none (a knob its type does not declare). */
export function handleAt(d: ModuleData, dir: 'in' | 'out', port: string): string | null {
  if (dir === 'out') return d.v2?.pout === port || d.v2?.mod === port || d.outs.some((o) => o.id === port) ? handleOf(d, 'out', port) : null;
  if (d.v2?.pin === port || d.ports.some((x) => x.id === port) || d.knobs.some((k) => k.id === port || (k.next && nextPort(k.id) === port) || (k.gain && gainPort(k.id) === port))) return handleOf(d, 'in', port);
  const poly = d.knobs.find((k) => k.channels?.some((c) => c.id === port));
  if (poly) return knobHandle(poly.id);
  // One channel of a knob whose input takes several (the flame's parts, `spread.3`): the knob's own socket.
  const whole = /^(.+)\.\d+$/.exec(port)?.[1];
  return whole && d.knobs.some((k) => k.id === whole && !k.channels) ? knobHandle(whole) : null;
}

/** The port a handle of a v2 node is. */
export function portOfHandle(d: ModuleData, handle: string): string | null {
  if (handle === 'in') return d.v2?.pin ?? null;
  if (handle === 'out') return d.v2?.pout ?? null;
  if (handle === MOD_OUT) return d.v2?.mod ?? null;
  if (handle.startsWith('k:') || handle.startsWith('o:')) return handle.slice(2);
  return null;
}

/** Where cables into a module's ports go at a level, by `module.port`. */
const intoOf = (p: PatchV2) => {
  const m = new Map<string, CableV2[]>();
  for (const c of p.cables) (m.get(`${c.to.module}.${c.to.port}`) ?? m.set(`${c.to.module}.${c.to.port}`, []).get(`${c.to.module}.${c.to.port}`)!).push(c);
  return m;
};

/** A music module of one signal takes the category of the first module it feeds, so ELK's partitions (and the lanes)
 *  put it beside what it feeds rather than in a far In / Out lane: short local cables. */
export function besideTargets(nodes: GraphNode[], edges: GraphEdge[]) {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  for (const n of nodes) {
    if (n.data.kind !== 'module' || !n.data.music) continue;
    const to = edges.find((e) => e.source === n.id && !e.data?.normal);
    const cat = to && (byId.get(to.target)?.data as ModuleData | undefined)?.category;
    if (cat) n.data = { ...n.data, category: cat };
  }
}

/** An edge drawn as its wire's type: a picture's or light's a wire (in steps), anything else a sagging cable; a normal
 *  faint. */
function wired(e: GraphEdge, wire: PortType): GraphEdge {
  const picture = PICTURE_TYPES.has(wire);
  const d = { ...e.data!, wire };
  if (picture) d.step = true;
  else delete d.step;
  return { ...e, type: picture ? 'wire' : 'cable', class: d.normal ? `normal w-${wire}` : picture ? wire : 'cable', style: wireStyle(wire), data: d };
}

/** A polymorphic output carries what comes into its node's picture input (VCV's polyphony: the output follows the
 *  input): a Fluid fed a picture gives a picture, a Feedback on light gives light. Its edges and its jacks by that type,
 *  followed along a chain of them. */
export function withCarried(nodes: GraphNode[], edges: GraphEdge[]) {
  const passing = nodes.filter((n) => {
    if (n.data.kind !== 'module') return false;
    const d = n.data;
    return !!d.v2.ports.in.find((x) => x.id === d.v2.pin)?.polymorphic && !!d.v2.ports.out.find((x) => x.id === d.v2.pout)?.polymorphic;
  });
  for (let pass = 0; pass < 16; pass++) {
    let changed = false;
    for (const n of passing) {
      const d = n.data as ModuleData;
      const out = d.v2.ports.out.find((x) => x.id === d.v2.pout)!;
      const t = edges.find((f) => f.target === n.id && f.targetHandle === 'in')?.data?.wire;
      if (!t || d.carried === t || !fits(t, out.type, out.polymorphic)) continue;
      n.data = { ...d, pinType: t, poutType: t, carried: t };
      for (let i = 0; i < edges.length; i++) if (edges[i].source === n.id && edges[i].sourceHandle === 'out' && edges[i].data?.wire !== t) edges[i] = wired(edges[i], t);
      changed = true;
    }
    if (!changed) break;
  }
}

/** The v2 patch as Svelte Flow's nodes and edges, at the level `at` leads to. */
export function graphOfV2(input: V2Input): { nodes: GraphNode[]; edges: GraphEdge[] } {
  if (!input.portrait || !input.view) return layoutV2(input, input.portrait ? PORTRAIT_ACROSS : LANE_ACROSS);
  // Portrait with the view known: the lanes' width whose whole fits it best (the largest fit zoom).
  const view = input.view;
  let best: { g: { nodes: GraphNode[]; edges: GraphEdge[] }; z: number } | null = null;
  for (let across = 400; across <= 2400; across += 200) {
    const g = layoutV2(input, across);
    const z = fitZoom(extent(g.nodes, (n) => input.sizes?.[n.id] ?? sizeOf(n.data)), view);
    if (!best || z > best.z + 1e-6) best = { g, z };
  }
  return best!.g;
}

function layoutV2({ patch, types, at = [], open = new Set(), positions = {}, sizes = {}, portrait = false }: V2Input, ACROSS: number): { nodes: GraphNode[]; edges: GraphEdge[] } {
  // Portrait: a far label's room between the modules.
  const BESIDE = portrait ? PORTRAIT_BESIDE : 0;
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const top = patchAt(patch, at);
  if (!top) return { nodes, edges };
  const portsFor = portsOfModule(types);
  // The patch as the engine runs it, for what a macro sets (0077): a node's flattened id is its path from the top.
  const flat = flatten(patch);
  const flatId = (id: string) => (at.length ? `${at.join('/')}/${id}` : id);
  const data = new Map<string, ModuleData>();

  /** A module's node data at a level (`into`: that level's cables by `module.port`). */
  const view = (m: Module, id: string, into: Map<string, CableV2[]>, level: PatchV2): ModuleData => {
    const declared = portsFor(m) ?? { in: [], out: [] };
    // A cable into a port its type does not declare (toV2's flame.zoom until it is): a port row of its own, so no cable
    // is hidden; a channel of a polyphonic knob or a knob's next or gain is its knob's.
    const known = new Set([...declared.in.map((x) => x.id), ...(types[m.type]?.knobs ?? []).map((k) => k.id)]);
    const stray = [...new Set(level.cables.filter((c) => c.to.module === m.id).map((c) => c.to.port))]
      .filter((p) => !known.has(p) && !known.has(p.replace(/\.\d+$/, '')) && !known.has(p.replace(/:(next|gain)$/, '')))
      .map((p): Port => ({ id: p, label: p.charAt(0).toUpperCase() + p.slice(1), type: 'value' }));
    const ports = stray.length ? { ...declared, in: [...declared.in, ...stray] } : declared;
    // A knob's cables, one channel's of it too (`spread.3` into the flame's Spread, whose input takes six).
    const channelsOf = (port: string) => [...into.entries()].filter(([k]) => k.startsWith(`${m.id}.${port}.`) && /^\d+$/.test(k.slice(m.id.length + port.length + 2))).flatMap(([, cs]) => cs);
    const cablesInto = (port: string) => [...(into.get(`${m.id}.${port}`) ?? []), ...channelsOf(port)];
    const cabled = (port: string) => cablesInto(port).length > 0;
    const pinPort = ports.in.find((x) => PICTURE_TYPES.has(x.type));
    const poutPort = ports.out.find((x) => PICTURE_TYPES.has(x.type));
    const values = ports.out.filter((x) => !PICTURE_TYPES.has(x.type));
    const modulator = !pinPort && !poutPort;
    let knobs: KnobView[] = [];
    let hidden = 0;
    let knobIds = new Set<string>();
    let label = m.type;
    let place: Place = modulator ? 'modulator' : 'module';
    if (isDevice(m)) {
      label = m.label;
      // Its exposed knobs, then its macros, all shown.
      for (const e of m.inputs) {
        const b = knobBehind(m, e.as, types);
        if (!b) continue;
        knobIds.add(e.as);
        const at = b.knob.max > b.knob.min ? clamp01((b.value - b.knob.min) / (b.knob.max - b.knob.min)) : 0;
        knobs.push({ id: e.as, label: e.label, value: b.value, min: b.knob.min, max: b.knob.max, kind: b.knob.kind, at, ring: ringOf(at, cablesInto(e.as)) });
      }
      for (const x of m.macros) {
        knobIds.add(macroKnob(x.id));
        // Mapped by a macro one level up (0077): as that sets it, marked so.
        const g = mappingOf(flat, macroNode(flatId(id), x.id), 'value');
        const value = g ? g.value : x.value;
        knobs.push({ id: macroKnob(x.id), label: x.label, value, min: 0, max: 1, kind: 'level', at: clamp01(value), ring: ringOf(clamp01(value), cablesInto(macroKnob(x.id))), ...(g ? { macro: { label: g.label, span: [clamp01(g.from), clamp01(g.to)] as [number, number] } } : {}) });
      }
    } else {
      const t = types[m.type] ?? { type: m.type, label: m.type, place, knobs: [] };
      label = moduleName(m, level, types);
      place = t.place;
      knobIds = new Set([...t.knobs, ...polyKnobs(t).knobs].map((k) => k.id));
      const shown = knobViews({ ...m, id }, t, { open, room: modulator ? 0 : SHOWN, cabled, cablesInto, mapped: (knob) => mappingOf(flat, flatId(id), knob, types) });
      knobs = shown.knobs.map((k) => (level.pins.includes(`${m.id}.${k.id}`) ? { ...k, pinned: true } : k));
      hidden = shown.hidden;
    }
    const parts = isDevice(m) ? [] : faceOf(types[m.type], declared);
    const gate = into.get(`${m.id}.gate`) ?? [];
    const lanes = Math.max(1, ...gate.map((c) => (c.from.channel === undefined ? (portsFor(level.modules.find((x) => x.id === c.from.module) ?? m)?.out.find((o) => o.id === c.from.port)?.channels ?? 1) : 1)));
    const d: ModuleData = {
      kind: 'module',
      module: id,
      type: m.type,
      label,
      place,
      knobs,
      hidden,
      ports: ports.in
        .filter((x) => !knobIds.has(x.id) && !isNext(x.id) && x !== pinPort)
        .map((x) => ({ id: x.id, label: x.label, type: x.type, ...(x.channels ? { channels: x.channels } : {}), cabled: cabled(x.id), ...(x.normal ? { normal: x.normal } : {}), ...(x.polymorphic ? { polymorphic: x.polymorphic } : {}) })),
      gives: modulator && values[0] ? { type: values[0].type, ...(values[0].channels ? { channels: values[0].channels } : {}) } : null,
      outs: values.map((o, i) => ({ id: o.id, label: o.label, type: o.type, handle: modulator && i === 0 ? MOD_OUT : outHandle(o.id) })),
      face: parts.length ? { parts, knobs: faceKnobs(m, types[m.type] ?? { type: m.type, label: m.type, place, knobs: [] }), lanes: parts.some((x) => x.kind === 'adsr') ? lanes : 1 } : null,
      in: pinPort ? 'left' : null,
      out: poutPort ? 'right' : null,
      ...(pinPort ? { pinType: pinPort.type } : {}),
      ...(poutPort ? { poutType: poutPort.type } : {}),
      modulator,
      v2: { ports, pin: pinPort?.id ?? null, pout: poutPort?.id ?? null, mod: modulator ? (values[0]?.id ?? null) : null },
      ...(isDevice(m) ? { device: { open: false } } : {}),
      ...(level.pins.includes(`module:${m.id}`) ? { pinned: true } : {}),
      ...(signalOf(m) ? { music: signalOf(m)!.id } : {}),
    };
    data.set(id, d);
    return d;
  };

  type Placed = { id: string; node: GraphNode; size: Size };
  /** One level: its modules laid out by lane from `origin`, inside `parent` where it is a device's frame; its extent. */
  const level = (p: PatchV2, prefix: string, parent: string | undefined, origin: XY): Size => {
    const into = intoOf(p);
    const items = new Map<string, Placed>();
    for (const m of p.modules) {
      const id = prefix + m.id;
      if (isDevice(m) && open.has(id)) {
        const box: GraphNode = { id, type: 'box', position: { x: 0, y: 0 }, data: { kind: 'box', label: m.label, device: true }, deletable: false, selectable: false, connectable: false, zIndex: -1 };
        if (parent) Object.assign(box, { parentId: parent, extent: 'parent', expandParent: true });
        nodes.push(box);
        const inner = level(m.patch, `${id}/`, id, { x: PAD, y: BOX_TOP });
        box.width = inner.width + 2 * PAD;
        box.height = inner.height + BOX_TOP + PAD;
        items.set(m.id, { id, node: box, size: { width: box.width, height: box.height } });
        continue;
      }
      const d = view(m, id, into, p);
      // The Screen is never deleted, so never picked.
      const node: GraphNode = { id, type: 'patch', position: { x: 0, y: 0 }, data: d, deletable: false, ...(m.type === 'screen' ? { selectable: false } : {}) };
      if (parent) Object.assign(node, { parentId: parent, extent: 'parent', expandParent: true });
      nodes.push(node);
      items.set(m.id, { id, node, size: sizes[id] ?? sizeOf(d) });
    }
    // By category (0057 §3b; rack.ts byCategory): a lane each, its header first (its name, a pin for the whole of it,
    // a "+" for a module of it; none inside a device's frame, which carries the device's name), its modules in a row under it (the effects in the order pictures flow) that turns into
    // another past LANE_ACROSS.
    const lanes = byCategory(p, types);
    const placed = new Set(CATEGORIES.flatMap((c) => lanes[c].map((m) => m.id)));
    lanes.io.push(...p.modules.filter((m) => !placed.has(m.id)));
    const put = (it: Placed, x: number, y: number) => (it.node.position = positions[it.id] ?? { x: origin.x + x, y: origin.y + y });
    const scope = prefix.split('/').filter(Boolean);
    let w = 0;
    let y = 0;
    for (const cat of CATEGORIES) {
      const ms = lanes[cat].map((m) => items.get(m.id)).filter((it): it is Placed => !!it);
      if (!ms.length) continue;
      const gap = Math.max(cat === 'effects' ? GAP_X : 24, BESIDE);
      let x = 0;
      let rowH = 0;
      for (const it of ms) {
        it.node.data = { ...it.node.data, category: cat } as NodeData;
        if (x > 0 && x + it.size.width > ACROSS) {
          x = 0;
          y += rowH + 40;
          rowH = 0;
        }
        put(it, x, y);
        x += it.size.width + gap;
        rowH = Math.max(rowH, it.size.height);
        w = Math.max(w, x - gap);
      }
      y += rowH + 56;
    }
    const h = Math.max(0, y - 56);

    // The cables at this level, each end led into a device opened in place to the module behind its port.
    const end = (r: PortRef, dir: 'in' | 'out', q: PatchV2 = p, pre = prefix): { node: string; port: string } | null => {
      const m = q.modules.find((x) => x.id === r.module);
      if (!m) return null;
      const id = pre + m.id;
      if (isDevice(m) && open.has(id)) {
        const e = (dir === 'in' ? m.inputs : m.outputs).find((x) => x.as === r.port);
        return e ? end({ module: e.module, port: e.port }, dir, m.patch, `${id}/`) : null;
      }
      return { node: id, port: r.port };
    };
    const edge = (id: string, from: { node: string; port: string }, to: { node: string; port: string }, c: Pick<CableV2, 'id' | 'value'> | null, channel?: number) => {
      const a = data.get(from.node);
      const b = data.get(to.node);
      const sourceHandle = a && handleAt(a, 'out', from.port);
      const targetHandle = b && handleAt(b, 'in', to.port);
      if (!a || !b || !sourceHandle || !targetHandle) return;
      const out = a.v2.ports.out.find((x) => x.id === from.port);
      const wire = out?.type ?? 'value';
      const channels = channel === undefined ? out?.channels : undefined;
      const cd: CableData = { amount: c ? amountLabel(c.value) : '', wire, ...(channels ? { channels } : {}), ...(c ? {} : { normal: true as const }), ...(c && p.pins.includes(`cable:${c.id}`) ? { pinned: true } : {}) };
      edges.push(wired({ id, source: from.node, target: to.node, sourceHandle, targetHandle, type: 'cable', class: '', deletable: !!c, selectable: !!c, data: cd }, wire));
    };
    for (const c of p.cables) {
      const a = end(c.from, 'out');
      const b = end(c.to, 'in');
      if (a && b) edge(prefix + c.id, a, b, c, c.from.channel);
    }
    // Each port's normal at this level while nothing is plugged into it.
    for (const m of p.modules) {
      const d = data.get(prefix + m.id);
      for (const port of d?.ports ?? []) {
        if (!port.normal || port.cabled) continue;
        const a = end({ module: port.normal.module, port: port.normal.port }, 'out');
        if (a) edge(`${prefix}normal:${m.id}.${port.id}`, a, { node: prefix + m.id, port: port.id }, null, port.normal.channel);
      }
    }
    return { width: w, height: h };
  };
  level(top, '', undefined, { x: 0, y: 0 });
  besideTargets(nodes, edges);
  withCarried(nodes, edges);
  withRoom(nodes, (n) => sizes[n.id] ?? sizeOf(n.data));
  withCounts(nodes, edges);
  withJacksOn(nodes, edges);
  return { nodes, edges };
}

/** A change made in the graph, for the store (patch-store.ts `change`): a knob turned (`module`: its path from the
 *  top; a device's exposed knob leads to the knob behind it, a macro's sets the macro), a cable drawn at a level
 *  (`scope`: the devices it is inside), cables taken out, a cable's end moved (its amount and depth kept), modules
 *  taken out (with every cable to or from them, and what of them a device exposes or a macro or a pin holds). */
export type V2Edit =
  | { kind: 'knob'; module: string[]; knob: string; value: number }
  // `replaces`: the cable a picture, light or points input had (it takes one: VCV's way, the new one in its place).
  | { kind: 'connect'; scope: string[]; from: PortRef; to: PortRef; replaces?: string }
  | { kind: 'remove'; scope: string[]; cables: string[] }
  | { kind: 'reroute'; scope: string[]; id: string; end: 'from' | 'to'; to: PortRef; replaces?: string }
  | { kind: 'removeModules'; scope: string[]; modules: string[] }
  | { kind: 'pinOne'; scope: string[]; pin: string; on: boolean }
  | { kind: 'pin'; scope: string[]; category: Category; on: boolean }
  | { kind: 'add'; scope: string[]; type: string; id?: string; knobs?: Record<string, number> }
  | { kind: 'make'; scope: string[]; modules: string[]; macros: MacroPick[]; label?: string }
  | { kind: 'unmake'; scope: string[]; device: string };

/** The store's key for an edit: a knob's turns one step. */
export const editKey = (e: V2Edit) => (e.kind === 'knob' ? `knob:${e.module.join('/')}.${e.knob}` : null);

/** An edit applied to `p` in place (a store's draft), and `p`. `types`: the knobs' ranges and names a device made
 *  takes for its macros and ports (device-make.ts). */
export function applyV2(p: PatchV2, e: V2Edit, types?: Record<string, ModuleType>): PatchV2 {
  if (e.kind === 'knob') {
    const inside = patchAt(p, e.module.slice(0, -1));
    const m = inside?.modules.find((x) => x.id === e.module[e.module.length - 1]);
    if (!m) return p;
    if (isDevice(m)) {
      if (e.knob.startsWith('macro:')) {
        const x = m.macros.find((y) => macroKnob(y.id) === e.knob);
        if (x) x.value = Math.min(1, Math.max(0, e.value));
        return p;
      }
      const ex = m.inputs.find((y) => y.as === e.knob);
      return ex ? applyV2(p, { kind: 'knob', module: [...e.module, ex.module], knob: ex.port, value: e.value }) : p;
    }
    m.knobs[e.knob] = e.value;
    return p;
  }
  const inside = patchAt(p, e.scope);
  if (!inside) return p;
  if (e.kind === 'pin') {
    const pin = categoryPin(e.category);
    inside.pins = inside.pins.filter((x) => x !== pin);
    if (e.on) inside.pins.push(pin);
    return p;
  }
  if (e.kind === 'make') {
    makeDevice(inside, e.modules, { label: e.label, macros: e.macros, types });
    return p;
  }
  if (e.kind === 'unmake') {
    unmakeDevice(inside, e.device);
    return p;
  }
  if (e.kind === 'add') {
    inside.modules.push({ id: e.id && !inside.modules.some((m) => m.id === e.id) ? e.id : freeModuleId(inside, e.type), type: e.type, knobs: { ...(e.knobs ?? {}) } });
    return p;
  }
  if (e.kind === 'remove') {
    const gone = new Set(e.cables);
    inside.cables = inside.cables.filter((c) => !gone.has(c.id));
    inside.pins = inside.pins.filter((x) => !(x.startsWith('cable:') && gone.has(x.slice(6))));
    return p;
  }
  if (e.kind === 'reroute') {
    // The one end moved (the panel's Reroute too: patch-edits.ts); the other, and the cable's amount, as they were.
    const c = inside.cables.find((x) => x.id === e.id);
    if (c) c[e.end] = { ...e.to };
    if (e.replaces && e.replaces !== e.id) inside.cables = inside.cables.filter((x) => x.id !== e.replaces);
    return p;
  }
  if (e.kind === 'pinOne') {
    // A knob (`<module>.<knob>`), a module or a device (`module:<id>`), a cable (`cable:<id>`): the panel's too.
    inside.pins = inside.pins.filter((x) => x !== e.pin);
    if (e.on) inside.pins.push(e.pin);
    return p;
  }
  if (e.kind === 'removeModules') {
    removeModules(p, e.scope, new Set(e.modules));
    return p;
  }
  if (e.replaces) inside.cables = inside.cables.filter((c) => c.id !== e.replaces);
  const taken = new Set(inside.cables.map((c) => c.id));
  let n = inside.cables.length + 1;
  while (taken.has(`cable-${n}`)) n++;
  inside.cables.push({ id: `cable-${n}`, from: e.from, to: e.to, value: 1 });
  return p;
}

/** Modules taken out of the patch at `scope`: their cables, the depths read from them, the macros' and pins' hold on
 *  them; where `scope` is a device's patch, the device's ports onto them and the cables outside into those. */
function removeModules(p: PatchV2, scope: readonly string[], gone: ReadonlySet<string>) {
  const inside = patchAt(p, scope);
  if (!inside) return;
  const cut = new Set(inside.cables.filter((c) => gone.has(c.from.module) || gone.has(c.to.module)).map((c) => c.id));
  inside.modules = inside.modules.filter((m) => !gone.has(m.id));
  inside.cables = inside.cables.filter((c) => !cut.has(c.id));
  for (const c of inside.cables) if (c.depth && gone.has(c.depth.module)) c.depth = null;
  for (const x of inside.macros) x.targets = x.targets.filter((t) => !gone.has(t.module.split('/')[0]));
  const held = (pin: string) => gone.has(pin.split(/[./]/)[0]) || (pin.startsWith('module:') && gone.has(pin.slice(7))) || (pin.startsWith('cable:') && cut.has(pin.slice(6)));
  inside.pins = inside.pins.filter((x) => !held(x));
  if (!scope.length) return;
  const outside = patchAt(p, scope.slice(0, -1));
  const device = outside?.modules.find((m) => m.id === scope[scope.length - 1]);
  if (!outside || !device || !isDevice(device)) return;
  for (const x of device.macros) x.targets = x.targets.filter((t) => !gone.has(t.module.split('/')[0]));
  const ins = new Set(device.inputs.filter((x) => gone.has(x.module)).map((x) => x.as));
  const outs = new Set(device.outputs.filter((x) => gone.has(x.module)).map((x) => x.as));
  device.inputs = device.inputs.filter((x) => !ins.has(x.as));
  device.outputs = device.outputs.filter((x) => !outs.has(x.as));
  if (!ins.size && !outs.size) return;
  outside.cables = outside.cables.filter((c) => !(c.to.module === device.id && ins.has(c.to.port)) && !(c.from.module === device.id && outs.has(c.from.port)));
}

/** What deleting picked modules and cables is, as one change: the cables at their levels, then the modules (their
 *  cables, pins and the rest with them: removeModules); never the Screen. Node and edge ids from the level `at` shows. */
export function deleteEdits(at: readonly string[], modules: readonly string[], cables: readonly string[], keep: (id: string) => boolean = () => true): V2Edit[] {
  const group = (ids: readonly string[]) => {
    const by = new Map<string, { scope: string[]; ids: string[] }>();
    for (const id of ids) {
      const parts = id.split('/');
      const scope = [...at, ...parts.slice(0, -1)];
      (by.get(scope.join('/')) ?? by.set(scope.join('/'), { scope, ids: [] }).get(scope.join('/'))!).ids.push(parts[parts.length - 1]);
    }
    return [...by.values()];
  };
  return [
    ...group(cables).map((g): V2Edit => ({ kind: 'remove', scope: g.scope, cables: g.ids })),
    ...group(modules.filter(keep)).map((g): V2Edit => ({ kind: 'removeModules', scope: g.scope, modules: g.ids })),
  ];
}

/** What the select bar offers for a selection (the owner: a long press on a module, a device or a cable is select
 *  mode): Delete (anything but the Screen), Pin, Make device (two or more modules at one level, no cables), Unmake
 *  (devices), Reroute (one cable alone), Open (one module or device alone), Help (one module alone, with help). */
export type BarAction = 'delete' | 'pin' | 'make' | 'unmake' | 'reroute' | 'open' | 'help';
export function barActions(modules: readonly { device?: boolean; screen?: boolean; help?: boolean; scope?: string }[], cables: number): BarAction[] {
  const out: BarAction[] = [];
  const n = modules.length + cables;
  if (!n) return out;
  out.push('pin');
  if (cables || modules.some((m) => !m.screen)) out.push('delete');
  if (!cables && modules.length > 1 && new Set(modules.map((m) => m.scope ?? '')).size === 1) out.push('make');
  if (modules.some((m) => m.device)) out.push('unmake');
  if (cables === 1 && !modules.length) out.push('reroute');
  if (!cables && modules.length === 1) out.push('open');
  if (!cables && modules.length === 1 && !modules[0].device && modules[0].help) out.push('help');
  return out;
}

/** Devices picked, unmade as one change (each as its menu's Unmake): node ids from the level `at` shows. */
export const unmakeEdits = (at: readonly string[], devices: readonly string[]): V2Edit[] =>
  devices.map((id) => {
    const parts = id.split('/');
    return { kind: 'unmake', scope: [...at, ...parts.slice(0, -1)], device: parts[parts.length - 1] };
  });

/** The cables' opacity (VCV Rack's, a view option kept on this device): 10 to 100 %, drawn as Rack draws it, its
 *  power 1.5 (nvgAlpha(pow(opacity, 1.5))). */
export const CABLE_OPACITY_MIN = 0.1;
export const cableOpacity = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? Math.min(1, Math.max(CABLE_OPACITY_MIN, v)) : 1);
export const drawnOpacity = (o: number) => Math.pow(cableOpacity(o), 1.5);

/** An id no module at this level has: `<base>-<n>`. */
export function freeModuleId(p: PatchV2, base: string): string {
  const taken = new Set(p.modules.map((m) => m.id));
  let n = 1;
  while (taken.has(`${base}-${n}`)) n++;
  return `${base}-${n}`;
}

/** The types a category's "+" offers (v2), by name. */
export const typesOf = (cat: Category, types: Record<string, ModuleType>): ModuleType[] =>
  Object.values(types)
    // Music's signals are its own entry (Music ▸), not a type each.
    .filter((t) => categoryOf(t) === cat && t.place !== 'frame' && t.type !== 'music' && t.type !== 'screen' && !signalOf({ type: t.type }))
    .sort((a, b) => labelOf(a).localeCompare(labelOf(b)));

/** What a connection drawn in the v2 graph makes, where it may: both ends at one level (the scope: the devices both
 *  are inside), an output into an input of a type that fits it, not one already there, a picture cycle only through a
 *  Feedback. `at`: the level shown. */
export function connectionV2(
  c: { source: string; target: string; sourceHandle?: string | null; targetHandle?: string | null },
  nodes: { id: string; data: NodeData }[],
  top: PatchV2,
  at: readonly string[],
  types: Record<string, ModuleType>,
  /** A cable left out of the checks (one being rerouted: its id at the connection's level). */
  without?: string,
): Extract<V2Edit, { kind: 'connect' }> | null {
  const a = nodes.find((n) => n.id === c.source)?.data;
  const b = nodes.find((n) => n.id === c.target)?.data;
  if (a?.kind !== 'module' || b?.kind !== 'module' || !a.v2 || !b.v2 || c.source === c.target) return null;
  const scope = (id: string) => id.split('/').slice(0, -1);
  if (scope(c.source).join('/') !== scope(c.target).join('/')) return null;
  const fromPort = portOfHandle(a, c.sourceHandle ?? '');
  const toPort = portOfHandle(b, c.targetHandle ?? '');
  const out = a.v2.ports.out.find((x) => x.id === fromPort);
  const into = b.v2.ports.in.find((x) => x.id === toPort);
  if (!out || !into) return null;
  // Out of a polymorphic output: what it carries where something comes in, else any of its types.
  const carried = out.id === a.v2.pout ? a.carried : undefined;
  if (!(carried ? fits(carried, into.type, into.polymorphic) : fits(out.type, into.type, into.polymorphic || out.polymorphic))) return null;
  const found = patchAt(top, scope(c.source));
  if (!found) return null;
  const inside = without === undefined ? found : { ...found, cables: found.cables.filter((x) => x.id !== without) };
  const from = { module: c.source.split('/').pop()!, port: out.id };
  const to = { module: c.target.split('/').pop()!, port: into.id };
  if (inside.cables.some((x) => x.from.module === from.module && x.from.port === from.port && x.to.module === to.module && x.to.port === to.port)) return null;
  // A picture, light or points input takes one cable: one already in it is replaced (VCV's way); a value or a
  // trigger input takes many.
  const single = PICTURE_TYPES.has(into.type) || into.type === 'points';
  const replaces = single ? inside.cables.find((x) => x.to.module === to.module && x.to.port === to.port)?.id : undefined;
  if (PICTURE_TYPES.has(out.type)) {
    const next: PatchV2 = { ...inside, cables: [...inside.cables.filter((x) => x.id !== replaces), { id: '?', from, to, value: 1 }] };
    const pictureOut = (t: string): (ModuleType & { ports?: Ports }) | undefined => (t === FEEDBACK.type ? FEEDBACK : types[t]);
    if (!cyclesClosed(next, pictureOut)) return null;
  }
  return { kind: 'connect', scope: [...at, ...scope(c.source)], from, to, ...(replaces ? { replaces } : {}) };
}

/** A cable's end moved in the graph (its edge as it was, by its id from the level shown; the connection it would be
 *  now): the reroute, checked as a new cable there would be (the cable itself left out); null where it cannot go, it
 *  leaves its level, or nothing moved. */
export function rerouteV2(
  id: string,
  now: { source: string; target: string; sourceHandle?: string | null; targetHandle?: string | null },
  nodes: { id: string; data: NodeData }[],
  top: PatchV2,
  at: readonly string[],
  types: Record<string, ModuleType>,
): Extract<V2Edit, { kind: 'reroute' }> | null {
  const cut = id.lastIndexOf('/');
  const scope = cut < 0 ? [] : id.slice(0, cut).split('/');
  const cable = cut < 0 ? id : id.slice(cut + 1);
  const was = patchAt(top, scope)?.cables.find((x) => x.id === cable);
  const made = was ? connectionV2(now, nodes, top, at, types, cable) : null;
  if (!was || !made || made.scope.join('/') !== [...at, ...scope].join('/')) return null;
  const fromMoved = made.from.module !== was.from.module || made.from.port !== was.from.port;
  const toMoved = made.to.module !== was.to.module || made.to.port !== was.to.port;
  if (fromMoved === toMoved) return null;
  return { kind: 'reroute', scope: made.scope, id: cable, end: fromMoved ? 'from' : 'to', to: fromMoved ? made.from : made.to, ...(made.replaces ? { replaces: made.replaces } : {}) };
}

/** Where a category's "+" adds: a module of it at a level (`scope`: the devices it is inside, from the level shown). */
export type AddTo = { category: Category; scope: string[] };

/** What PatchGraph hands its nodes and cables (Svelte context, under `GRAPH`). */
export type GraphActions = {
  /** A knob turned (as it turns); a polyphonic knob drawn as one, its channels turned together (`poly`: its id). */
  knob(module: string, knob: string, value: number): void;
  knobs(module: string, poly: string, values: { id: string; value: number }[]): void;
  open(id: string): void;
  /** Into a device (v2): its own modules as the level shown. */
  enter(id: string): void;
  /** A knob's or a port's long press: what it is (module-help.ts), where there is help for it. */
  help(node: string, port: string): void;
  /** A knob's fine adjust as it goes (null: let go). */
  fine(f: Fine | null): void;
  /** Open a node out to every knob, or back; a device (v2) opened in place, or closed. */
  toggle(id: string): void;
  opened(id: string): boolean;
  /** A knob tapped (not turned): what moves it. */
  openKnob(module: string, knob: string): void;
  /** A module's long press: its menu (Select, Delete; a device's Open and Unmake; its help). */
  menu(id: string): void;
  /** A category's "+" tapped: the types to add there. */
  adding(to: AddTo): void;
  /** A whole category pinned, or let go (v2: `category:<name>` in the pins of the patch at `scope`). */
  pinCategory(scope: string[], category: Category, on: boolean): void;
  /** A cable tapped (its amount, too): its configuration; its long press at a point (the window's), its menu. */
  openCable(id: string): void;
  cableMenu(id: string, at: XY): void;
  /** The cable being rerouted, and where its long press was (its nearer end is the one that moves). */
  rerouting(): { id: string; at: XY } | null;
  /** The live faces' frame (faces.ts), where the parent feeds one. */
  live(): import('./faces').LiveFrame | null;
  /** A node's module's path from the top (its devices, `/`): what its live values are found by. */
  livePath(id: string): string;
  /** The graph's thumbnails (face/face.ts Thumbs), where the parent draws them. */
  thumbs(): Thumbs | null;
  /** Whether the graph is laid out for a screen taller than wide (the far sockets then at top and bottom). */
  portrait(): boolean;
  /** Whether a modulation cable follows its ELK route (else it sags as VCV Rack's do). */
  routeCables(): boolean;
};
export const GRAPH = 'patchgraph';
