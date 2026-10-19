<script lang="ts">
  // The graph builder (decision 0056, step 3): the patch drawn with Svelte Flow, its picture cables (each layer into
  // the blend, the loop, finish, the screen) solid, its modulation cables (a signal or a modulator to a knob or a port)
  // dashed with their amounts. It never changes `patch`: every edit goes out through a callback and the parent hands a
  // new patch back.
  //
  // - onconnect(from, to): a cable drawn from a signal's or a modulator's output to a knob or a port (`to.knob` is the
  //   port's id); the parent makes the Cable (its id, value and mode).
  // - ondisconnect(cableId): a cable unplugged (its ×, or Delete).
  // - onknob(moduleId, knobId, value): a knob turned, as it turns.
  // - onmove(id, x, y): a node let go. `id` is a module's id, or `layer:<layer id>`, `@loop`, `@blend`, `@screen`,
  //   `signal:<key>`; a module inside a box is relative to its box. `positions` hands them back, keyed the same (0056
  //   keeps none in the patch); without it the graph keeps them itself while it is open.
  // - onopen(id): a node's title tapped, or the node double-clicked (ids as onmove's).
  import { setContext, untrack } from 'svelte';
  import { SvelteFlow, Panel, type Connection, type Edge, type Node } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/base.css';
  import PatchNode from './PatchNode.svelte';
  import PatchCable from './PatchCable.svelte';
  import type { Cable, ModuleType, Patch, SignalRef } from './patch';
  import { cableOf, graphOf, GRAPH, type GraphActions, type NodeData, type Size, type XY } from './patchgraph';

  let {
    patch,
    types,
    positions,
    signals = [],
    onconnect,
    ondisconnect,
    onknob,
    onmove,
    onopen,
  }: {
    patch: Patch;
    /** Module types by their type id. */
    types: Record<string, ModuleType>;
    positions?: Record<string, XY>;
    /** Signals shown though nothing is cabled from them yet. */
    signals?: SignalRef[];
    onconnect: (from: Cable['from'], to: Cable['to']) => void;
    ondisconnect: (cableId: string) => void;
    onknob: (moduleId: string, knobId: string, value: number) => void;
    onmove: (id: string, x: number, y: number) => void;
    onopen: (id: string) => void;
  } = $props();

  const nodeTypes = { patch: PatchNode, box: PatchNode };
  const edgeTypes = { cable: PatchCable };

  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);
  let opened = $state.raw<ReadonlySet<string>>(new Set());
  let moved = $state.raw<Record<string, XY>>({});
  const placed = $derived(positions ?? moved);

  // The nodes' measured sizes, as a key, so a measure that changes nothing lays nothing out again.
  const sizeKey = $derived(
    nodes
      .filter((n) => n.type === 'patch' && n.measured?.width)
      .map((n) => `${n.id}=${Math.round(n.measured!.width!)}x${Math.round(n.measured!.height!)}`)
      .join(';'),
  );

  $effect.pre(() => {
    const sizes: Record<string, Size> = {};
    for (const s of sizeKey ? sizeKey.split(';') : []) {
      const [id, wh] = [s.slice(0, s.lastIndexOf('=')), s.slice(s.lastIndexOf('=') + 1)];
      const [width, height] = wh.split('x').map(Number);
      sizes[id] = { width, height };
    }
    const g = graphOf({ patch, types, positions: placed, signals, open: opened, sizes });
    untrack(() => {
      // What Svelte Flow knows of each node (its size, selection, a drag under way) carries over.
      const before = new Map(nodes.map((n) => [n.id, n]));
      nodes = g.nodes.map((n) => {
        const b = before.get(n.id);
        return { ...n, measured: b?.measured, selected: b?.selected, ...(b?.dragging ? { position: b.position, dragging: true } : {}) } as Node;
      });
      const chosen = new Set(edges.filter((e) => e.selected).map((e) => e.id));
      edges = g.edges.map((e) => ({ ...e, selected: chosen.has(e.id) }) as Edge);
    });
  });

  setContext<GraphActions>(GRAPH, {
    knob: (module, knob, value) => onknob(module, knob, value),
    open: (id) => onopen(id),
    toggle(id) {
      const next = new Set(opened);
      if (!next.delete(id)) next.add(id);
      opened = next;
    },
    opened: (id) => opened.has(id),
    disconnect: (cable) => ondisconnect(cable),
  });

  const ends = () => nodes as unknown as { id: string; data: NodeData }[];
  const valid = (c: Connection | Edge) => cableOf(c, ends(), patch.cables) !== null;
  function connect(c: Connection) {
    const cable = cableOf(c, ends(), patch.cables);
    if (cable) onconnect(cable.from, cable.to);
    return null;
  }
  async function unplug({ edges: gone }: { nodes: Node[]; edges: Edge[] }) {
    for (const e of gone) if (e.type === 'cable') ondisconnect(e.id);
    return false;
  }
  function dropped({ nodes: released }: { nodes: Node[] }) {
    const next = { ...moved };
    for (const n of released) {
      next[n.id] = { x: Math.round(n.position.x), y: Math.round(n.position.y) };
      onmove(n.id, next[n.id].x, next[n.id].y);
    }
    if (!positions) moved = next;
  }
</script>

<div class="patch-graph">
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
    {edgeTypes}
    colorMode="dark"
    fitView
    minZoom={0.2}
    maxZoom={2}
    zoomOnDoubleClick={false}
    deleteKey={['Backspace', 'Delete']}
    isValidConnection={valid}
    onbeforeconnect={connect}
    onbeforedelete={unplug}
    onnodedragstop={dropped}
  >
    <Panel position="top-right">
      <div class="legend"><span><i></i>Picture</span><span><i class="m"></i>Modulation</span></div>
    </Panel>
  </SvelteFlow>
</div>

<style>
  .patch-graph {
    --patch-mod: #b48cff;
    --patch-bg: color-mix(in srgb, var(--bg) 92%, var(--text));
    width: 100%;
    height: 100%;
    background: var(--patch-bg);
    color: var(--text);
  }
  .patch-graph :global(.svelte-flow) {
    background: transparent;
    --xy-connectionline-stroke: var(--patch-mod);
    --xy-connectionline-stroke-width: 2;
  }
  .patch-graph :global(.svelte-flow__connection-path) {
    stroke-dasharray: 6 5;
  }
  .patch-graph :global(.svelte-flow__edge.picture .svelte-flow__edge-path),
  .patch-graph :global(.svelte-flow__edge.feedback .svelte-flow__edge-path) {
    stroke: var(--text);
    stroke-width: 2;
    fill: none;
    opacity: 0.75;
  }
  .patch-graph :global(.svelte-flow__edge.feedback .svelte-flow__edge-path) {
    opacity: 0.55;
    stroke-dasharray: 2 4;
  }
  .patch-graph :global(.svelte-flow__edge.cable .svelte-flow__edge-path) {
    stroke: var(--patch-mod);
    stroke-width: 2;
    stroke-dasharray: 6 5;
    fill: none;
    opacity: 0.9;
  }
  .patch-graph :global(.svelte-flow__edge.cable.selected .svelte-flow__edge-path) {
    stroke-width: 3;
    opacity: 1;
  }
  .patch-graph :global(.svelte-flow__edge-label) {
    background: var(--patch-bg);
    padding: 0 4px;
    border-radius: 4px;
    font-size: 11.5px;
    color: var(--muted);
  }
  .patch-graph :global(.svelte-flow__attribution) {
    background: transparent;
    font-size: 10px;
  }
  .patch-graph :global(.svelte-flow__attribution a) {
    color: var(--muted);
  }
  .legend {
    display: flex;
    gap: 16px;
    align-items: center;
    color: var(--muted);
    font-size: 12.5px;
  }
  .legend i {
    display: inline-block;
    width: 24px;
    margin-right: 6px;
    vertical-align: middle;
    border-top: 2px solid var(--text);
  }
  .legend i.m {
    border-top: 2px dashed var(--patch-mod);
  }
</style>
