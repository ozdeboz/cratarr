// The graph's layout by ELK (the Eclipse Layout Kernel's `layered`: Sugiyama's layers, crossings minimised, ports kept
// in order, edges routed orthogonally), the owner's choice for the graph (0057 §7b). elkjs 0.12.0 is "EPL-2.0 OR
// GPL-3.0-or-later" (its files' SPDX lines and Exhibit A); Cratarr uses it under GPL-3.0-or-later, combined into its own
// AGPL-3.0-or-later. 0.11.0 was EPL-2.0 alone, hence 0.12.0.
//
// Pure but for the ELK instance handed in (the page's runs in a web worker, the tests' in-process):
// - toElk: the graph's nodes (a frame round a device opened in place a compound node, its modules inside) and their
//   cables' sockets as ports, inputs west and outputs east (where the jacks are, in portrait too), in their order; laid
//   by signal flow alone (the owner, 71.1: no lanes, as VCV Rack).
// - fromElk: ELK's places for the nodes (a node the viewer put somewhere stays there), a frame's size, and each picture
//   wire's route (its bends; its ends are its jacks' centres, PatchWire).
// - laneHeads: each category's header over its lane's nodes once ELK has placed them.

import type { ELK, ElkExtendedEdge, ElkNode, ElkPort } from 'elkjs/lib/elk-api';
import type { Category } from './rack';
import { MOD_OUT, withRoom, type GraphEdge, type GraphNode, type NodeData, type Size, type XY } from './patchgraph';

/** `before`: where the last layout put each node (relative to its frame). Given, ELK keeps to it (its interactive
 *  layering and crossing minimisation, which read the nodes' places): the layout takes in a change without being made
 *  again, a node new to it put by its cables' other ends. */
export type ElkOptions = { portrait?: boolean; before?: (id: string) => XY | undefined };

/** A port's ELK id: its node's and its handle's. */
const portId = (node: string, handle: string) => `${node}::${handle}`;

/** A category's partition, in the order signals flow (so ELK's layers run with the cables, not against them): what
 *  comes in (In: Music, the Filter bank; an In / Out module that only gives), the Modulators, the Sources, the Effects,
 *  the Mixers, what goes out (Out: the Screen, the whole frame). The lanes keep the five categories' names. */
export function flowOrder(c: Category, handles: readonly string[]): number {
  if (c === 'io') return handles.length && handles.every((h) => !isInput(h)) ? 0 : 5;
  return { modulators: 1, sources: 2, effects: 3, mixers: 4 }[c];
}

/** Which of a node's handles take cables in (the rest give them out). */
function isInput(handle: string): boolean {
  return handle === 'in' || handle === 'fbin' || handle.startsWith('k:');
}

/** The graph as ELK takes it. Lane headers are not ELK's (laneHeads puts them). */
export function toElk(nodes: GraphNode[], edges: GraphEdge[], size: (n: GraphNode) => Size, o: ElkOptions = {}): ElkNode {
  // Where the jacks are (PatchNode): inputs down a node's left, outputs down its right, on a phone too.
  const inSide = 'WEST';
  const outSide = 'EAST';
  const laid = nodes.filter((n) => n.data.kind !== 'lane');
  const ids = new Set(laid.map((n) => n.id));
  // Each node's ports: the handles its cables use, in the order they come.
  const used = new Map<string, string[]>();
  const use = (node: string, handle: string | null | undefined) => {
    if (!handle) return;
    const list = used.get(node) ?? used.set(node, []).get(node)!;
    if (!list.includes(handle)) list.push(handle);
  };
  const drawn = edges.filter((e) => ids.has(e.source) && ids.has(e.target));
  for (const e of drawn) {
    use(e.source, e.sourceHandle);
    use(e.target, e.targetHandle);
  }
  const elkNode = (n: GraphNode): ElkNode => {
    const s = n.width && n.height ? { width: n.width, height: n.height } : size(n);
    const handles = used.get(n.id) ?? [];
    const ports: ElkPort[] = [...handles.filter(isInput), ...handles.filter((h) => !isInput(h))].map((h, i) => ({
      id: portId(n.id, h),
      width: 10,
      height: 10,
      layoutOptions: { 'elk.port.side': isInput(h) ? inSide : outSide, 'elk.port.index': String(i) },
    }));
    const children = laid.filter((c) => c.parentId === n.id).map(elkNode);
    return {
      id: n.id,
      ...placedBefore(n),
      ...(children.length ? {} : s),
      ports,
      ...(children.length ? { children } : {}),
      layoutOptions: {
        'elk.portConstraints': 'FIXED_ORDER',
        // A frame's room round its modules: its name over them, the same room as between modules inside.
        ...(children.length ? { 'elk.padding': '[top=48,left=28,bottom=28,right=28]', 'elk.spacing.nodeNode': '48', 'elk.layered.spacing.nodeNodeBetweenLayers': '72' } : {}),
      },
    };
  };
  // Where the last layout had each node; a new one beside what it is cabled to (after what feeds it, else before what it
  // feeds), on the flow's axis.
  const before = o.before;
  const along = o.portrait ? 'y' : 'x';
  const long = o.portrait ? 'height' : 'width';
  const gap = 96;
  const sized = (id: string) => {
    const n = laid.find((x) => x.id === id);
    return n ? (n.width && n.height ? { width: n.width, height: n.height } : size(n)) : { width: 0, height: 0 };
  };
  const hinted = (n: GraphNode): XY | undefined => {
    const had = before?.(n.id);
    if (had) return had;
    const ends = (mine: 'source' | 'target', other: 'source' | 'target') =>
      drawn.filter((e) => e[mine] === n.id && before?.(e[other])).map((e) => ({ at: before!(e[other])!, id: e[other] }));
    const from = ends('target', 'source');
    const to = ends('source', 'target');
    // Cabled to nothing laid yet: at the start of the flow, past everything across it.
    if (!from.length && !to.length) {
      const had = laid.map((x) => ({ at: before?.(x.id), id: x.id })).filter((x): x is { at: XY; id: string } => !!x.at);
      if (!had.length) return undefined;
      const cross = along === 'x' ? 'y' : 'x';
      const start = Math.min(...had.map((x) => x.at[along]));
      const past = Math.max(...had.map((x) => x.at[cross] + sized(x.id)[along === 'x' ? 'height' : 'width'])) + gap;
      return along === 'x' ? { x: start, y: past } : { x: past, y: start };
    }
    // Between what feeds it and what it feeds where there is room, else just before what it feeds (a modulator beside
    // what it moves), else just after what feeds it.
    const after = from.length ? Math.max(...from.map((f) => f.at[along] + sized(f.id)[long])) + gap : -Infinity;
    const until = to.length ? Math.min(...to.map((t) => t.at[along])) - gap - sized(n.id)[long] : Infinity;
    const at = after <= until ? (Number.isFinite(after) && Number.isFinite(until) ? (after + until) / 2 : Number.isFinite(after) ? after : until) : until;
    const across = (to[0] ?? from[0]).at[along === 'x' ? 'y' : 'x'];
    return along === 'x' ? { x: at, y: across } : { x: across, y: at };
  };
  const placedBefore = (n: GraphNode): Partial<XY> => (before ? (hinted(n) ?? {}) : {});
  const elkEdges: ElkExtendedEdge[] = drawn.map((e) => ({ id: e.id, sources: [portId(e.source, e.sourceHandle ?? MOD_OUT)], targets: [portId(e.target, e.targetHandle ?? 'in')] }));
  return {
    id: '@root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': o.portrait ? 'DOWN' : 'RIGHT',
      'elk.edgeRouting': 'ORTHOGONAL',
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      // Back edges (a Feedback's last frame, a loop's way back) routed round, not through.
      'elk.layered.feedbackEdges': 'true',
      ...(before
        ? {
            'elk.layered.layering.strategy': 'INTERACTIVE',
            'elk.layered.crossingMinimization.strategy': 'INTERACTIVE',
            'elk.layered.cycleBreaking.strategy': 'INTERACTIVE',
            'elk.layered.nodePlacement.strategy': 'INTERACTIVE',
          }
        : { 'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP' }),
      // Room between modules shown whole (their real sizes, whatever the zoom), and for the cables between.
      'elk.spacing.nodeNode': '48',
      'elk.layered.spacing.nodeNodeBetweenLayers': '96',
      'elk.spacing.edgeNode': '24',
      'elk.spacing.edgeEdge': '12',
      'elk.spacing.componentComponent': '72',
      // Every route in the root's coordinates (the flow's).
      'elk.json.edgeCoords': 'ROOT',
      'elk.json.shapeCoords': 'PARENT',
    },
    children: laid.filter((n) => !n.parentId || !ids.has(n.parentId)).map(elkNode),
    edges: elkEdges,
  };
}

/** ELK's layout as the graph's: each node's place (relative to its frame, as Svelte Flow has it) but where the viewer
 *  put it (`kept`), a frame's size, each wire's bends (`data.route`, the flow's coordinates; not `withRoutes`: none, as
 *  while a layout of a changed graph is on its way). */
export function fromElk(laid: ElkNode, nodes: GraphNode[], edges: GraphEdge[], kept: (id: string) => boolean, size?: (n: GraphNode) => Size, withRoutes = true): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const at = new Map<string, ElkNode>();
  const walk = (n: ElkNode) => {
    at.set(n.id, n);
    for (const c of n.children ?? []) walk(c);
  };
  walk(laid);
  const routes = new Map<string, XY[]>();
  const collect = (n: ElkNode) => {
    for (const e of n.edges ?? []) {
      const s = e.sections?.[0];
      if (s) routes.set(e.id, [s.startPoint, ...(s.bendPoints ?? []), s.endPoint]);
    }
    for (const c of n.children ?? []) collect(c);
  };
  collect(laid);
  const placed = nodes.map((n) => {
    const e = at.get(n.id);
    if (!e || n.data.kind === 'lane') return n;
    const next: GraphNode = { ...n };
    if (!kept(n.id) && e.x !== undefined && e.y !== undefined) next.position = { x: Math.round(e.x), y: Math.round(e.y) };
    if (n.type === 'box' && e.width && e.height) Object.assign(next, { width: Math.round(e.width), height: Math.round(e.height) });
    return next;
  });
  const moved = new Set(nodes.filter((n) => kept(n.id)).map((n) => n.id));
  const routed = edges.map((e) => {
    const r = routes.get(e.id);
    if (!withRoutes || !r || moved.has(e.source) || moved.has(e.target)) return e;
    return { ...e, data: { ...(e.data ?? { amount: '', wire: 'picture' as const }), route: r } };
  });
  const out = laneHeads(placed);
  // The far labels' room as ELK placed the nodes.
  if (size) withRoom(out, size);
  return { nodes: out, edges: routed };
}

/** Each category's header (a lane node, `@lane:<category>`) just above its lane's topmost node as placed (the
 *  leftmost of those level with it). */
export function laneHeads(nodes: GraphNode[]): GraphNode[] {
  const box = new Map<Category, { x: number; y: number }>();
  for (const n of nodes) {
    const c = (n.data as { category?: Category }).category;
    if (!c || n.parentId || n.data.kind === 'lane') continue;
    const b = box.get(c);
    if (!b || n.position.y < b.y - 0.5 || (Math.abs(n.position.y - b.y) <= 0.5 && n.position.x < b.x)) box.set(c, { ...n.position });
  }
  return nodes.map((n) => {
    const d = n.data as NodeData;
    if (d.kind !== 'lane' || n.parentId) return n;
    const b = box.get(d.category);
    return b ? { ...n, position: { x: b.x, y: b.y - 34 } } : n;
  });
}

/** A layout laid from `before` (toElk's), moved back by what its nodes moved as one (their median), so the graph on the
 *  screen stays where it was but for what the change itself moved. In place; the routes with it. */
export function anchored(laid: ElkNode, before: (id: string) => XY | undefined): ElkNode {
  const dx: number[] = [];
  const dy: number[] = [];
  for (const c of laid.children ?? []) {
    const b = before(c.id);
    if (b && c.x !== undefined && c.y !== undefined) {
      dx.push(c.x - b.x);
      dy.push(c.y - b.y);
    }
  }
  if (!dx.length) return laid;
  const median = (v: number[]) => v.sort((a, b) => a - b)[v.length >> 1];
  const [x, y] = [median(dx), median(dy)];
  for (const c of laid.children ?? []) {
    if (c.x !== undefined) c.x -= x;
    if (c.y !== undefined) c.y -= y;
  }
  const back = (q: { x: number; y: number }) => ((q.x -= x), (q.y -= y));
  const routes = (n: ElkNode) => {
    for (const e of n.edges ?? [])
      for (const sec of e.sections ?? []) {
        back(sec.startPoint);
        back(sec.endPoint);
        sec.bendPoints?.forEach(back);
      }
    n.children?.forEach(routes);
  };
  routes(laid);
  return laid;
}

/** What a layout depends on (not a knob's value): the nodes, their sizes and frames, the cables' ends. ELK runs again only
 *  when it changes. */
export function layoutKey(nodes: GraphNode[], edges: GraphEdge[], size: (n: GraphNode) => Size, o: ElkOptions = {}): string {
  const ns = nodes.map((n) => {
    const s = size(n);
    return `${n.id}:${n.parentId ?? ''}:${Math.round(s.width)}x${Math.round(s.height)}`;
  });
  const es = edges.map((e) => `${e.source}.${e.sourceHandle}>${e.target}.${e.targetHandle}`);
  return `${o.portrait ? 'p' : 'l'}|${ns.join(',')}|${es.join(',')}`;
}

/** The layout ELK makes of the graph (asynchronously: the page's ELK runs in a worker). */
export async function elkLayout(elk: Pick<ELK, 'layout'>, nodes: GraphNode[], edges: GraphEdge[], size: (n: GraphNode) => Size, kept: (id: string) => boolean, o: ElkOptions = {}) {
  const laid = await elk.layout(toElk(nodes, edges, size, o));
  return fromElk(laid as ElkNode, nodes, edges, kept, size);
}

