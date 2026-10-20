// The graph's layout by ELK (the Eclipse Layout Kernel's `layered`: Sugiyama's layers, crossings minimised, ports kept
// in order, edges routed orthogonally), the owner's choice for the graph (0057 §7b). elkjs 0.12.0 is "EPL-2.0 OR
// GPL-3.0-or-later" (its files' SPDX lines and Exhibit A); Cratarr uses it under GPL-3.0-or-later, combined into its own
// AGPL-3.0-or-later. 0.11.0 was EPL-2.0 alone, hence 0.12.0.
//
// Pure but for the ELK instance handed in (the page's runs in a web worker, the tests' in-process):
// - toElk: the graph's nodes (a frame round a device opened in place a compound node, its modules inside) and their
//   cables' sockets as ports, inputs on the side the picture comes in (west, or north in portrait) and outputs on the
//   other, in their order; the categories as ELK partitions, so their lanes stay in order.
// - fromElk: ELK's places for the nodes (a node the viewer put somewhere stays there), a frame's size, and each picture
//   wire's route (its bends; its ends are its jacks' centres, PatchWire).
// - laneHeads: each category's header over its lane's nodes once ELK has placed them.

import type { ELK, ElkExtendedEdge, ElkNode, ElkPort } from 'elkjs/lib/elk-api';
import { CATEGORIES, type Category } from './rack';
import { FAR_IN, MOD_OUT, type GraphEdge, type GraphNode, type NodeData, type Size, type XY } from './patchgraph';

export type ElkOptions = { portrait?: boolean };

/** A port's ELK id: its node's and its handle's. */
const portId = (node: string, handle: string) => `${node}::${handle}`;

/** Which of a node's handles take cables in (the rest give them out). */
function isInput(handle: string): boolean {
  return handle === 'in' || handle === 'fbin' || handle === FAR_IN || handle.startsWith('k:');
}

/** The graph as ELK takes it. Lane headers are not ELK's (laneHeads puts them). */
export function toElk(nodes: GraphNode[], edges: GraphEdge[], size: (n: GraphNode) => Size, o: ElkOptions = {}): ElkNode {
  const inSide = o.portrait ? 'NORTH' : 'WEST';
  const outSide = o.portrait ? 'SOUTH' : 'EAST';
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
    const category = (n.data as { category?: Category }).category;
    const children = laid.filter((c) => c.parentId === n.id).map(elkNode);
    return {
      id: n.id,
      ...(children.length ? {} : s),
      ports,
      ...(children.length ? { children } : {}),
      layoutOptions: {
        'elk.portConstraints': 'FIXED_ORDER',
        ...(category ? { 'elk.partitioning.partition': String(CATEGORIES.indexOf(category)) } : {}),
        ...(children.length ? { 'elk.padding': '[top=34,left=16,bottom=16,right=16]' } : {}),
      },
    };
  };
  const elkEdges: ElkExtendedEdge[] = drawn.map((e) => ({ id: e.id, sources: [portId(e.source, e.sourceHandle ?? MOD_OUT)], targets: [portId(e.target, e.targetHandle ?? FAR_IN)] }));
  return {
    id: '@root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': o.portrait ? 'DOWN' : 'RIGHT',
      'elk.edgeRouting': 'ORTHOGONAL',
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      'elk.partitioning.activate': 'true',
      // Back edges (a Feedback's last frame, a loop's way back) routed round, not through.
      'elk.layered.feedbackEdges': 'true',
      'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
      'elk.spacing.nodeNode': '36',
      'elk.layered.spacing.nodeNodeBetweenLayers': '72',
      'elk.spacing.edgeNode': '18',
      'elk.spacing.edgeEdge': '10',
      // Every route in the root's coordinates (the flow's).
      'elk.json.edgeCoords': 'ROOT',
      'elk.json.shapeCoords': 'PARENT',
    },
    children: laid.filter((n) => !n.parentId || !ids.has(n.parentId)).map(elkNode),
    edges: elkEdges,
  };
}

/** ELK's layout as the graph's: each node's place (relative to its frame, as Svelte Flow has it) but where the viewer
 *  put it (`kept`), a frame's size, each wire's bends (`data.route`, the flow's coordinates). */
export function fromElk(laid: ElkNode, nodes: GraphNode[], edges: GraphEdge[], kept: (id: string) => boolean): { nodes: GraphNode[]; edges: GraphEdge[] } {
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
    if (!r || moved.has(e.source) || moved.has(e.target)) return e;
    return { ...e, data: { ...(e.data ?? { amount: '', wire: 'picture' as const }), route: r } };
  });
  return { nodes: laneHeads(placed), edges: routed };
}

/** Each category's header (a lane node, `@lane:<category>`) just above its lane's nodes as placed. */
export function laneHeads(nodes: GraphNode[]): GraphNode[] {
  const box = new Map<Category, { x: number; y: number }>();
  for (const n of nodes) {
    const c = (n.data as { category?: Category }).category;
    if (!c || n.parentId) continue;
    const b = box.get(c);
    box.set(c, b ? { x: Math.min(b.x, n.position.x), y: Math.min(b.y, n.position.y) } : { ...n.position });
  }
  return nodes.map((n) => {
    const d = n.data as NodeData;
    if (d.kind !== 'lane' || n.parentId) return n;
    const b = box.get(d.category);
    return b ? { ...n, position: { x: b.x, y: b.y - 34 } } : n;
  });
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
  return fromElk(laid as ElkNode, nodes, edges, kept);
}

