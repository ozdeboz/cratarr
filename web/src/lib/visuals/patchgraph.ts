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
import { cyclesClosed, FEEDBACK, fits, isDevice, MUSIC, polyKnobs, portsOf, type CableV2, type Device, type ExposedPort, type PatchV2, type Port, type PortRef, type PortType, type Ports } from './ports';
import { byCategory, CATEGORIES, CATEGORY_LABEL, categoryOf, type Category } from './rack';
import { FACES, type FaceKind } from './faces';
import { freeCableId, newCable, type Cables } from './cable-actions';
import { makeDevice, unmakeDevice, type MacroPick } from './device-make';

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
  /** A polyphonic knob drawn as one (0057 §4b: a band's six): each channel's knob id, value and place in the range; the
   *  knob's own `value` and `at` their mean. */
  channels?: { id: string; value: number; at: number }[];
  /** A step knob's `<id>:next` trigger (0057 §7e), drawn as a small diamond beside it; a level knob's `<id>:gain`
   *  input (§7e), a small dot beside it. */
  next?: boolean;
  gain?: boolean;
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
  /** Where a loop's way back leaves and comes in (portrait: the right; else under). */
  fbSide?: Side;
  /** How far right of its left edge the node's far form may reach before the next node (flow px). */
  room?: number;
  /** How many cables come in and go out (its far form says so). */
  cables?: Count;
  /** Its category's lane (v2): ELK keeps the lanes in order by it. */
  category?: Category;
  modulator: boolean;
  /** Drawn from a v2 patch (graphOfV2): its ports whole, which of them the picture handles and the `mod` handle are,
   *  every handle connectable. */
  v2?: { ports: Ports; pin: string | null; pout: string | null; mod: string | null };
  /** A device (0057 §6): opened in place (drawn as a frame round its modules instead). */
  device?: { open: boolean };
};
/** A typed input port on a node (not a knob): cabled, or else normalled where its type says so. */
export type PortView = { id: string; label: string; type: PortType; channels?: number; cabled: boolean; normal?: PortRef; polymorphic?: boolean };
/** An output jack: its handle (`mod`, or `o:<port>`). */
export type OutView = { id: string; label: string; type: PortType; handle: string; connectable?: boolean };
/** A signal of Music's: its type (an event's a trigger, the sound audio), a band's six at once `channels` 6. */
export type SignalData = { kind: 'signal'; label: string; ref: SignalRef; type: PortType; channels?: number; room?: number; cables?: Count };
export type MarkData = { kind: 'blend' | 'screen'; label: string; in: Side; out: Side | null; room?: number; cables?: Count };
/** Cables into a node and out of it. */
export type Count = { in: number; out: number };
/** A layer's box, or the loop's; `add`: where a module added from its "+" goes (the layer's id, or `loop`). */
export type BoxData = { kind: 'box'; label: string; add?: string; device?: true; category?: Category };
/** A layer that is off, as one row: what brings it back is its id (the flame's: `flame`). */
export type OffData = { kind: 'off'; layer: string; label: string };
/** "+ Add layer" at the picture lane's end, "+ Add modulator" by the modulators. */
export type AddData = { kind: 'add'; to: 'layer' | 'modulator'; label: string };
/** A category's lane's header (v2): its name, whether the whole category is pinned, the devices it is inside. */
export type LaneData = { kind: 'lane'; category: Category; label: string; pinned: boolean; scope: string[] };
export type NodeData = ModuleData | SignalData | MarkData | BoxData | OffData | AddData | LaneData;

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
export type CableData = { amount: string; wire: PortType; channels?: number; lit?: boolean; muted?: true; normal?: true; step?: boolean; route?: XY[] };

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
export function openView(b: Rect, view: Size, pad: Pad, zoom = FULL_ZOOM): { x: number; y: number; zoom: number } {
  const w = view.width - pad.left - pad.right;
  const h = view.height - pad.top - pad.bottom;
  const axis = (start: number, extent: number, room: number, lead: number) => (extent * zoom <= room ? lead + (room - extent * zoom) / 2 - start * zoom : lead - start * zoom);
  return { x: axis(b.x, b.width, w, pad.left), y: axis(b.y, b.height, h, pad.top), zoom };
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
/** Down the picture's column (portrait). */
const GAP_Y = 44;
/** How wide a lane's rows run (portrait, v2), and the room between its modules for a far label. */
const PORTRAIT_ACROSS = 900;
const PORTRAIT_BESIDE = 150;
/** Between the picture's column and the modulators' (portrait): room for a far label beside each. */
const PORTRAIT_GAP = 200;
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
  return (t.type.endsWith('-engine') && ENGINES.find((e) => e.key === key)?.label) || RENAMED[t.type] || t.label;
}
/** Names the graph gives ahead of their types (0057 §7e: the course-and-swing filter is Accent; "flash" is the Screen's). */
const RENAMED: Record<string, string> = { flash: 'Accent' };

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
/** Output jacks a row. */
export const JACKS_ACROSS = 6;
export const LANE_H = 11;
export function faceHeight(f: ModuleData['face']): number {
  if (!f || f.kind === 'clock') return 0;
  if (f.kind === 'random') return 26;
  if (f.kind === 'vca' || f.kind === 'merge') return 18;
  return f.kind === 'adsr' && f.lanes > 1 ? f.lanes * LANE_H : 26;
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
  o: { open: ReadonlySet<string>; room: number; cabled: (knob: string) => boolean; cablesInto: (knob: string) => Pick<Cable, 'value' | 'mode'>[] },
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
    const beside = { ...(hasNext(k) ? { next: true } : {}), ...(hasGain(k) ? { gain: true } : {}) };
    if (!k.channels) {
      const value = m.knobs[k.id] ?? k.default;
      return { id: k.id, label: k.label, value, min: k.min, max: k.max, kind: k.kind, at: atOf(k, value), ring: ring(atOf(k, value)), ...beside };
    }
    const channels = channelKnobs(k).map((x) => {
      const value = m.knobs[x.id] ?? m.knobs[k.id] ?? x.default;
      return { id: x.id, value, at: atOf(k, value) };
    });
    const value = channels.reduce((a, c) => a + c.value, 0) / channels.length;
    return { id: k.id, label: k.label, value, min: k.min, max: k.max, kind: k.kind, at: atOf(k, value), ring: ring(atOf(k, value)), channels, ...beside };
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

/** Portrait (0057 §7b): each group of top-level nodes (the picture's, in its order; the modulators beside it) laid in
 *  columns, a column turning past a height chosen so the whole fits `view` best (the largest fit zoom); nodes the viewer
 *  put somewhere (`kept`) stay. */
export function fitPortrait(nodes: GraphNode[], size: (n: GraphNode) => Size, view: Size, groups: string[][], kept: (id: string) => boolean) {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const sz = (id: string) => {
    const n = byId.get(id)!;
    return { width: n.width ?? size(n).width, height: n.height ?? size(n).height };
  };
  const lists = groups.map((g) => g.filter((id) => byId.has(id) && !kept(id)));
  const place = (most: number, apply: boolean): Size => {
    let x = 0;
    let bottom = 0;
    for (const list of lists) {
      if (!list.length) continue;
      let y = 0;
      let w = 0;
      for (const id of list) {
        const s = sz(id);
        if (y > 0 && y + s.height > most) {
          x += w + PORTRAIT_GAP;
          y = 0;
          w = 0;
        }
        if (apply) byId.get(id)!.position = { x, y };
        y += s.height + GAP_Y;
        w = Math.max(w, s.width);
        bottom = Math.max(bottom, y - GAP_Y);
      }
      x += w + PORTRAIT_GAP;
    }
    return { width: x - PORTRAIT_GAP, height: bottom };
  };
  const all = lists.flat();
  if (!all.length) return;
  const tallest = Math.max(...all.map((id) => sz(id).height));
  const total = Math.max(...lists.map((l) => l.reduce((a, id) => a + sz(id).height + GAP_Y, 0)));
  let best = { most: total, z: fitZoom(place(total, false), view) };
  for (let i = 0; i <= 16; i++) {
    const most = tallest + ((total - tallest) * i) / 16;
    const z = fitZoom(place(most, false), view);
    if (z > best.z + 1e-6) best = { most, z };
  }
  place(best.most, true);
}

/** Each node's cables in and out (not its normals): its far form counts them. */
export function withCounts(nodes: GraphNode[], edges: GraphEdge[]) {
  const count = new Map<string, Count>();
  const at = (id: string) => count.get(id) ?? count.set(id, { in: 0, out: 0 }).get(id)!;
  for (const e of edges) {
    if (e.data?.normal || e.class === 'feedback') continue;
    at(e.source).out++;
    at(e.target).in++;
  }
  for (const n of nodes) if (n.data.kind === 'module' || n.data.kind === 'signal' || n.data.kind === 'blend' || n.data.kind === 'screen') n.data = { ...n.data, cables: count.get(n.id) ?? { in: 0, out: 0 } };
}
/** A node's cables as its far form says them: "6 in · 12 out", one side alone where the other has none. */
export function countLabel(c: Count | undefined): string {
  if (!c) return '';
  return [c.in ? `${c.in} in` : '', c.out ? `${c.out} out` : ''].filter(Boolean).join(' · ');
}

/** The far form's sockets (0057 §7b): one for what comes in, on the side the node takes in, one for what goes out, on
 *  the side it gives out; far out every cable is led to these (bundled there), so none ends inside a node. */
export const FAR_IN = 'far-in';
export const FAR_OUT = 'far-out';
/** The edges as the far form draws them: each end on its node's far socket (a frame's edges, which have none, as they
 *  are). */
export function farEdges<E extends { source: string; target: string; sourceHandle?: string | null; targetHandle?: string | null }>(edges: E[], nodes: { id: string; data: NodeData }[]): E[] {
  const socketed = new Set(nodes.filter((n) => n.data.kind === 'module' || n.data.kind === 'signal' || n.data.kind === 'blend' || n.data.kind === 'screen').map((n) => n.id));
  return edges.map((e) => ({ ...e, ...(socketed.has(e.source) ? { sourceHandle: FAR_OUT } : {}), ...(socketed.has(e.target) ? { targetHandle: FAR_IN } : {}) }));
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
    if (a.n.data.kind !== 'module' && a.n.data.kind !== 'signal' && a.n.data.kind !== 'blend' && a.n.data.kind !== 'screen') continue;
    let room = Math.min(ROOM_MOST, right - a.x);
    for (const b of boxes) if (b !== a && b.x > a.x && b.y < a.y + a.h && b.y + b.h > a.y) room = Math.min(room, b.x - a.x - 12);
    a.n.data = { ...a.n.data, room: Math.max(a.w, room) };
  }
}

/** A node's size before it is measured, from what it shows. */
export function sizeOf(d: NodeData): Size {
  if (d.kind === 'off') return { width: 200, height: 36 };
  if (d.kind === 'add') return { width: 130, height: 32 };
  if (d.kind === 'lane') return { width: 220, height: 26 };
  if (d.kind !== 'module') return d.kind === 'box' ? { width: 0, height: 0 } : { width: d.kind === 'signal' ? 150 : d.kind === 'blend' ? 84 : 120, height: 36 };
  const shown = d.knobs.length + (d.hidden ? 1 : 0);
  const rows = Math.ceil(shown / KNOBS_ACROSS);
  return {
    width: Math.max(d.modulator ? 150 : 140, 20 + Math.min(shown, KNOBS_ACROSS) * KNOB_W),
    height: 38 + rows * 50 + d.ports.length * 20 + Math.ceil(d.outs.length / JACKS_ACROSS) * 30 + (faceHeight(d.face) ? faceHeight(d.face) + 8 : 0),
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
  /** A screen taller than wide (a phone upright): the picture runs down, the modulators in columns beside it. */
  portrait?: boolean;
  /** The graph's visible size (above an open sheet): the portrait layout takes the shape that fits it best. */
  view?: Size;
};

/** Whether a layer is off: blended at nothing, so none of it is seen. */
export const layerOff = (l: Pick<Patch['layers'][number], 'blend'>) => l.blend.amount <= 0;

/** The patch as Svelte Flow's nodes and edges. The picture runs left to right: each layer a row in its box (its own
 *  loop at the row's end), the layers stacked, then the blend, the loop, finish and the screen in one row; the frame
 *  under the screen. Signals and modulators sit under the knobs they move, so their cables run short and mostly up. */
export function graphOf({ patch, types, positions = {}, signals = [], open = new Set(), sizes = {}, adds = false, addTo = () => true, portrait = false, view }: GraphInput): { nodes: GraphNode[]; edges: GraphEdge[] } {
  // The way the picture runs: across, or (portrait) down.
  const IN: Side = portrait ? 'top' : 'left';
  const OUT: Side = portrait ? 'bottom' : 'right';
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
    // Its knobs' ids, a polyphonic knob's own among them (its port is the knob's, not another input).
    const own = new Set([...t.knobs, ...polyKnobs(t).knobs].map((k) => k.id));
    const modulator = t.place === 'modulator';
    const cabled = (k: string) => into.has(`${m.id}.${k}`);
    const { knobs, hidden } = knobViews(m, t, {
      open,
      room: modulator ? 0 : SHOWN,
      cabled,
      cablesInto: (k) => (into.get(`${m.id}.${k}`) ?? []).filter((c) => !muted(c)),
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
      hidden,
      ports: declared.in
        .filter((p) => !own.has(p.id) && !isNext(p.id) && p.type !== 'picture' && p.type !== 'light')
        .map((p) => ({ id: p.id, label: p.label, type: p.type, ...(p.channels ? { channels: p.channels } : {}), cabled: cabled(p.id), ...(p.normal ? { normal: p.normal } : {}), ...(p.polymorphic ? { polymorphic: true } : {}) })),
      gives: outs[0] ? (modulator ? portOf({ modulator: m.id }) : { type: outs[0].type }) : null,
      outs: outs.length > 1 ? outs.map((o, i) => ({ id: o.id, label: o.label, type: o.type, handle: i ? outHandle(o.id) : MOD_OUT })) : [],
      face: kind ? { kind, knobs: faceKnobs(m, t), lanes: kind === 'adsr' ? lanesInto(m.id, 'gate') : 1 } : null,
      in: flow.in,
      out: flow.out,
      feedback: !!flow.feedback,
      feedbackIn: !!flow.feedbackIn,
      ...(portrait ? { fbSide: 'right' as const } : {}),
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
    edges.push({ id: `pic:${a}>${b}`, source: a, target: b, sourceHandle: 'out', targetHandle: 'in', type: 'wire', class: wire, style: wireStyle(wire), deletable: false, selectable: false, data: { amount: '', wire }, ...(label ? { label } : {}) });
  };

  /** A box around a row of modules, those from `loopAt` on a loop (its last back to its first). */
  const chain = (boxId: string, label: string, row: Module[], loopAt: number, firstIn: Side | null, origin: XY, add?: string) => {
    const data: BoxData = { kind: 'box', label, ...(adds && add && addTo(add) ? { add } : {}) };
    const box: GraphNode = { id: boxId, type: 'box', position: put(boxId, origin), data, deletable: false, selectable: false, connectable: false, zIndex: -1 };
    nodes.push(box);
    const inside: GraphNode[] = [];
    const loops = row.length - loopAt > 1;
    let x = PAD;
    let y = BOX_TOP;
    row.forEach((m, i) => {
      const flow: Flow = { in: i === 0 ? firstIn : IN, out: OUT, feedback: loops && i === row.length - 1, feedbackIn: loops && i === loopAt };
      const n = node(m.id, moduleData(m, flow), { x, y }, boxId);
      inside.push(n);
      if (portrait) y += size(n).height + GAP_Y;
      else x += size(n).width + GAP_X;
    });
    for (let i = 1; i < row.length; i++) picture(row[i - 1].id, row[i].id);
    if (loops)
      edges.push({ id: `fb:${row.at(-1)!.id}>${row[loopAt].id}`, source: row.at(-1)!.id, target: row[loopAt].id, sourceHandle: 'fb', targetHandle: 'fbin', type: 'wire', class: 'feedback', style: wireStyle(gives.get(row.at(-1)!.id) ?? 'picture'), label: 'feedback', deletable: false, selectable: false, data: { amount: '', wire: gives.get(row.at(-1)!.id) ?? 'picture', step: true } });
    box.width = Math.max(120, ...inside.map((n) => n.position.x + size(n).width)) + PAD;
    box.height = Math.max(60, ...inside.map((n) => n.position.y + size(n).height)) + PAD + (loops && !portrait ? 24 : 0);
    if (loops && portrait) box.width += 32;
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

  // The blend, the picture's loop, finish and the screen: in a row level with the layers' middle, or (portrait) on
  // down under them.
  const mid = Math.max(0, bottom / 2 - 18);
  let x = right + GAP_X;
  let down = bottom + 48;
  /** Where the next part goes, and past it. */
  const at = (dy = 0): XY => (portrait ? { x: 0, y: down } : { x, y: Math.max(0, mid + dy) });
  const past = (n: GraphNode) => {
    if (portrait) down = n.position.y + (n.height ?? size(n).height) + GAP_Y;
    else x = n.position.x + (n.width ?? size(n).width) + GAP_X;
    bottom = Math.max(bottom, n.position.y + (n.height ?? size(n).height));
  };
  const blend = node(BLEND, { kind: 'blend', label: 'Blend', in: IN, out: OUT }, at());
  past(blend);
  for (const f of toBlend) picture(f.id, BLEND, f.label);
  let from = BLEND;
  if (patch.loop.length) {
    const { box, last } = chain(LOOP, 'Loop', patch.loop, 0, IN, at(-BOX_TOP - 4), 'loop');
    picture(BLEND, patch.loop[0].id);
    from = last!;
    past(box);
  }
  for (const m of patch.finish) {
    const n = node(m.id, moduleData(m, { in: IN, out: OUT }), at(-4));
    picture(from, m.id);
    from = m.id;
    past(n);
  }
  const screen = node(SCREEN, { kind: 'screen', label: 'Screen', in: IN, out: null }, at());
  picture(from, SCREEN);
  past(screen);
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
  // Portrait: in columns beside the picture's, as tall as it, in the order of what they move (so the whole is about
  // as tall as wide as a phone's screen above a sheet, and a far label has the room beside it); `fitPortrait` then
  // turns both into the columns that fit the view best.
  const pictureColumn = nodes.filter((n) => !n.parentId).map((n) => n.id);
  const column = { x: Math.max(...nodes.filter((n) => !n.parentId).map((n) => n.position.x + (n.width ?? size(n).width)), 0) + PORTRAIT_GAP, y: 0, w: 0 };
  const tall = Math.max(bottom, 600);
  for (const x of [...first, ...later]) {
    if (later.includes(x)) x.want = toPort(x);
    const sz = sizes[x.id] ?? sizeOf(x.data);
    if (portrait) {
      if (column.y > 0 && column.y + sz.height > tall) {
        column.x += column.w + PORTRAIT_GAP;
        column.y = 0;
        column.w = 0;
      }
      node(x.id, x.data, { x: column.x, y: column.y });
      placedAt.set(x.id, column.x);
      column.y += sz.height + GAP_Y;
      column.w = Math.max(column.w, sz.width);
      continue;
    }
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
  if (portrait) sy = 0;
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
    const n = node(ADD_MODULATOR, { kind: 'add', to: 'modulator', label: '+ Add modulator' }, portrait ? { x: column.x, y: column.y } : { x: Math.round(first.length ? Math.min(...first) : 0), y: sy });
    Object.assign(n, { selectable: false, connectable: false, draggable: false });
  }

  // The modulation cables, where both ends are drawn, each its source's wire (a trigger into a knob stays a trigger's).
  const ids = new Set(nodes.map((n) => n.id));
  const modules = new Map(nodes.filter((n) => n.data.kind === 'module').map((n) => [n.id, n.data as ModuleData]));
  for (const c of patch.cables) {
    const source = 'modulator' in c.from ? c.from.modulator : signalKey(c.from);
    const target = modules.get(c.to.module);
    if (!ids.has(source) || !target) continue;
    // Into a channel of a polyphonic knob drawn as one: onto that knob.
    const knob = target.knobs.find((k) => k.id === c.to.knob) ?? target.knobs.find((k) => k.channels?.some((x) => x.id === c.to.knob));
    const next = target.knobs.some((k) => (k.next && nextPort(k.id) === c.to.knob) || (k.gain && gainPort(k.id) === c.to.knob));
    if (!knob && !next && !target.ports.some((p) => p.id === c.to.knob)) continue;
    const from = portOf(c.from);
    const data: CableData = { amount: amountLabel(c.value, c.mode), wire: from.type, ...(from.channels ? { channels: from.channels } : {}), ...(muted(c) ? { muted: true as const } : {}) };
    edges.push({ id: c.id, source, target: c.to.module, sourceHandle: MOD_OUT, targetHandle: knobHandle(next ? c.to.knob : (knob?.id ?? c.to.knob)), type: 'cable', class: 'cable', style: wireStyle(from.type, muted(c)), deletable: true, selectable: true, data });
  }
  for (const n of normals) {
    if (!ids.has(n.from) || !ids.has(n.to)) continue;
    const data: CableData = { amount: '', wire: n.wire, normal: true };
    edges.push({ id: `normal:${n.to}.${n.port}`, source: n.from, target: n.to, sourceHandle: n.handle, targetHandle: knobHandle(n.port), type: 'cable', class: `normal w-${n.wire}`, style: wireStyle(n.wire), deletable: false, selectable: false, data });
  }
  if (portrait && view) {
    const beside = nodes.filter((n) => !n.parentId && !pictureColumn.includes(n.id)).map((n) => n.id);
    fitPortrait(nodes, size, view, [pictureColumn, beside], (id) => !!positions[id]);
  }
  withRoom(nodes, size);
  withCounts(nodes, edges);
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
  const port = dst.ports.find((p) => p.id === to.knob);
  if (given === 'audio' || !fits(given, port?.type ?? (to.knob.endsWith(':next') ? 'trigger' : 'value'), port?.polymorphic)) return null;
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

/** A polyphonic knob drawn as one, turned to `value`: each channel moved as far, so they keep their differences
 *  (each clamped to the range). */
export function polyTurned(k: Pick<KnobView, 'value' | 'min' | 'max' | 'kind' | 'channels'>, value: number): { id: string; value: number }[] {
  const d = value - k.value;
  return (k.channels ?? []).map((c) => {
    const v = Math.min(k.max, Math.max(k.min, c.value + d));
    return { id: c.id, value: k.kind === 'step' ? Math.round(v) : v };
  });
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

// ---- A v2 patch drawn (0057 §6, §7): modules and cables between typed ports, devices as nodes ------------------------
// The patch as the store holds it (patch-store.ts), drawn level by level: the level shown is the one `at` leads to
// (the devices entered, from the top); a device there is one node with its exposed ports and its macros as knobs,
// or, opened in place (its id in `open`), a frame round its own modules and cables, nested as deep as devices go. Node
// ids are paths from the level shown (`show/bend`). The lanes are rack.ts's: the sources down the left, the picture's
// modules in the order pictures flow, the output after them, the control below in rows. Edits come back as V2Edit
// (applyV2), so the store can keep each one as a step. The app's own graph is still v1's (graphOf) until a5's step.

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

/** A module's ports: its type's, or a device's from what it exposes (each exposed port the type of the one behind it,
 *  through nested devices; a channel of a bundle as one). */
export function portsOfModule(types: Record<string, ModuleType>): (m: Module) => Ports | null {
  const self = (m: Module): Ports | null => {
    if (!isDevice(m)) {
      const t = types[m.type];
      return t ? portsOf(t) : null;
    }
    const behind = (e: ExposedPort, dir: 'in' | 'out'): Port | undefined => {
      const inner = m.patch.modules.find((x) => x.id === e.module);
      return inner ? self(inner)?.[dir].find((x) => x.id === e.port) : undefined;
    };
    const exposed = (e: ExposedPort, dir: 'in' | 'out', or: PortType): Port => {
      const b = behind(e, dir);
      return {
        id: e.as,
        label: e.label,
        type: b?.type ?? or,
        ...(b?.channels && e.channel === undefined ? { channels: b.channels } : {}),
        ...(b?.polymorphic ? { polymorphic: true } : {}),
      };
    };
    return { in: m.inputs.map((e) => exposed(e, 'in', 'value')), out: m.outputs.map((e) => exposed(e, 'out', 'picture')) };
  };
  return self;
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
  return poly ? knobHandle(poly.id) : null;
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
    const cabled = (port: string) => into.has(`${m.id}.${port}`);
    const cablesInto = (port: string) => into.get(`${m.id}.${port}`) ?? [];
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
        knobs.push({ id: macroKnob(x.id), label: x.label, value: x.value, min: 0, max: 1, kind: 'level', at: clamp01(x.value), ring: ringOf(clamp01(x.value), cablesInto(macroKnob(x.id))) });
      }
    } else {
      const t = types[m.type] ?? { type: m.type, label: m.type, place, knobs: [] };
      label = labelOf(t);
      place = t.place;
      knobIds = new Set([...t.knobs, ...polyKnobs(t).knobs].map((k) => k.id));
      const shown = knobViews({ ...m, id }, t, { open, room: modulator ? 0 : SHOWN, cabled, cablesInto });
      knobs = shown.knobs;
      hidden = shown.hidden;
    }
    const kind = isDevice(m) ? undefined : FACES[m.type];
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
        .map((x) => ({ id: x.id, label: x.label, type: x.type, ...(x.channels ? { channels: x.channels } : {}), cabled: cabled(x.id), ...(x.normal ? { normal: x.normal } : {}), ...(x.polymorphic ? { polymorphic: true } : {}) })),
      gives: modulator && values[0] ? { type: values[0].type, ...(values[0].channels ? { channels: values[0].channels } : {}) } : null,
      outs: (modulator ? (values.length > 1 ? values : []) : values).map((o, i) => ({ id: o.id, label: o.label, type: o.type, handle: modulator && i === 0 ? MOD_OUT : outHandle(o.id), connectable: true })),
      face: kind ? { kind, knobs: faceKnobs(m, types[m.type] ?? { type: m.type, label: m.type, place, knobs: [] }), lanes: kind === 'adsr' ? lanes : 1 } : null,
      in: pinPort ? 'left' : null,
      out: poutPort ? 'right' : null,
      feedback: false,
      feedbackIn: false,
      modulator,
      v2: { ports, pin: pinPort?.id ?? null, pout: poutPort?.id ?? null, mod: modulator ? (values[0]?.id ?? null) : null },
      ...(isDevice(m) ? { device: { open: false } } : {}),
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
      const node: GraphNode = { id, type: 'patch', position: { x: 0, y: 0 }, data: d, deletable: false };
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
      if (!parent) {
        const hid = `${prefix}${laneId(cat)}`;
        const head: GraphNode = {
          id: hid,
          type: 'patch',
          position: { x: origin.x, y: origin.y + y },
          data: { kind: 'lane', category: cat, label: CATEGORY_LABEL[cat], pinned: p.pins.includes(categoryPin(cat)), scope },
          deletable: false,
          selectable: false,
          connectable: false,
          draggable: false,
        };
        nodes.push(head);
        y += LANE_HEAD;
      }
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
    const edge = (id: string, from: { node: string; port: string }, to: { node: string; port: string }, c: Pick<CableV2, 'value' | 'mode'> | null, channel?: number) => {
      const a = data.get(from.node);
      const b = data.get(to.node);
      const sourceHandle = a && handleAt(a, 'out', from.port);
      const targetHandle = b && handleAt(b, 'in', to.port);
      if (!a || !b || !sourceHandle || !targetHandle) return;
      const out = a.v2!.ports.out.find((x) => x.id === from.port);
      const wire = out?.type ?? 'value';
      const channels = channel === undefined ? out?.channels : undefined;
      const picture = PICTURE_TYPES.has(wire);
      const cd: CableData = { amount: c ? amountLabel(c.value, c.mode) : '', wire, ...(channels ? { channels } : {}), ...(c ? {} : { normal: true as const }), ...(picture ? { step: true } : {}) };
      edges.push({
        id,
        source: from.node,
        target: to.node,
        sourceHandle,
        targetHandle,
        type: picture ? ('wire' as const) : ('cable' as const),
        class: !c ? `normal w-${wire}` : picture ? wire : 'cable',
        style: wireStyle(wire),
        deletable: !!c,
        selectable: !!c,
        data: cd,
      });
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
  withRoom(nodes, (n) => sizes[n.id] ?? sizeOf(n.data));
  withCounts(nodes, edges);
  return { nodes, edges };
}

/** A change made in the v2 graph, for the store (patch-store.ts `change`): a knob turned (`module`: its path from the
 *  top; a device's exposed knob leads to the knob behind it, a macro's sets the macro), a cable drawn at a level
 *  (`scope`: the devices it is inside), cables taken out. */
export type V2Edit =
  | { kind: 'knob'; module: string[]; knob: string; value: number }
  | { kind: 'connect'; scope: string[]; from: PortRef; to: PortRef }
  | { kind: 'remove'; scope: string[]; cables: string[] }
  | { kind: 'pin'; scope: string[]; category: Category; on: boolean }
  | { kind: 'add'; scope: string[]; type: string }
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
    const taken = new Set(inside.modules.map((m) => m.id));
    let n = 1;
    while (taken.has(`${e.type}-${n}`)) n++;
    inside.modules.push({ id: `${e.type}-${n}`, type: e.type, knobs: {} });
    return p;
  }
  if (e.kind === 'remove') {
    const gone = new Set(e.cables);
    inside.cables = inside.cables.filter((c) => !gone.has(c.id));
    return p;
  }
  const taken = new Set(inside.cables.map((c) => c.id));
  let n = inside.cables.length + 1;
  while (taken.has(`cable-${n}`)) n++;
  inside.cables.push({ id: `cable-${n}`, from: e.from, to: e.to, value: 1, mode: 'swing' });
  return p;
}

/** The types a category's "+" offers (v2), by name. */
export const typesOf = (cat: Category, types: Record<string, ModuleType>): ModuleType[] =>
  Object.values(types)
    .filter((t) => categoryOf(t) === cat && t.place !== 'frame' && t.type !== 'music' && t.type !== 'screen')
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
  if (!out || !into || !fits(out.type, into.type, into.polymorphic)) return null;
  const inside = patchAt(top, scope(c.source));
  if (!inside) return null;
  const from = { module: c.source.split('/').pop()!, port: out.id };
  const to = { module: c.target.split('/').pop()!, port: into.id };
  if (inside.cables.some((x) => x.from.module === from.module && x.from.port === from.port && x.to.module === to.module && x.to.port === to.port)) return null;
  if (PICTURE_TYPES.has(out.type)) {
    const next: PatchV2 = { ...inside, cables: [...inside.cables, { id: '?', from, to, value: 1, mode: 'swing' }] };
    const pictureOut = (t: string): (ModuleType & { ports?: Ports }) | undefined => (t === FEEDBACK.type ? FEEDBACK : types[t]);
    if (!cyclesClosed(next, pictureOut)) return null;
  }
  return { kind: 'connect', scope: [...at, ...scope(c.source)], from, to };
}

/** Where a "+" adds: a layer, a modulator, a module into a box (v1); a module of a category at a level (v2). */
export type AddTo = 'layer' | 'modulator' | { box: string } | { category: Category; scope: string[] };

/** What PatchGraph hands its nodes and cables (Svelte context, under `GRAPH`). */
export type GraphActions = {
  knob(module: string, knob: string, value: number): void;
  open(id: string): void;
  /** Into a device (v2): its own modules as the level shown. */
  enter(id: string): void;
  /** Open a node out to every knob, or back; a device (v2) opened in place, or closed. */
  toggle(id: string): void;
  opened(id: string): boolean;
  /** A knob tapped (not turned): what moves it. */
  openKnob(module: string, knob: string): void;
  /** Whether a node can be removed, and its long press (its menu: Delete). */
  removable(id: string): boolean;
  menu(id: string): void;
  /** A "+" tapped: the types to add there (a layer, a modulator, a module into a box: its `add`; v2, a category's). */
  adding(to: AddTo): void;
  /** A whole category pinned, or let go (v2: `category:<name>` in the pins of the patch at `scope`). */
  pinCategory(scope: string[], category: Category, on: boolean): void;
  /** A layer that is off, brought back. */
  bringBack(layer: string): void;
  /** A cable tapped (its amount, too): its configuration; its long press at a point (the window's), its menu. */
  openCable(id: string): void;
  cableMenu(id: string, at: XY): void;
  /** The cable being rerouted, and where its long press was (its nearer end is the one that moves). */
  rerouting(): { id: string; at: XY } | null;
  /** The live faces' frame (faces.ts), where the parent feeds one. */
  live(): import('./faces').LiveFrame | null;
  /** Whether the graph is laid out for a screen taller than wide (the far sockets then at top and bottom). */
  portrait(): boolean;
  /** Whether a modulation cable follows its ELK route (else it sags as VCV Rack's do). */
  routeCables(): boolean;
};
export const GRAPH = 'patchgraph';
