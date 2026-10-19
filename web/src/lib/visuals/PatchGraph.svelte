<script lang="ts">
  // The graph builder (decision 0056, step 3): the patch drawn with Svelte Flow, its picture cables (each layer into
  // the blend, the loop, finish, the screen) solid, its modulation cables (a signal or a modulator to a knob or a port)
  // dashed and sagging (after VCV Rack's) with their amounts. It never changes `patch`: every edit goes out through a
  // callback and the parent hands a new patch back.
  //
  // - onconnect(from, to): a cable drawn from a signal's or a modulator's output to a knob or a port (`to.knob` is the
  //   port's id); the parent makes the Cable (its id, value and mode).
  // - ondisconnect(cableId): a cable unplugged with Delete.
  // - onknob(moduleId, knobId, value): a knob turned, as it turns.
  // - onmove(id, x, y): a node let go. `id` is a module's id, or `layer:<layer id>`, `@loop`, `@blend`, `@screen`,
  //   `signal:<key>`; a module inside a box is relative to its box. `positions` hands them back, keyed the same (0056
  //   keeps none in the patch); without it the graph keeps them itself while it is open.
  // - onopen(id): a node's title tapped, or the node double-clicked (ids as onmove's).
  // - onopencable(id): a cable (or its amount) tapped: its configuration. The graph marks it as the open one.
  // - onopenknob(module, knob): a knob tapped, not turned: what moves it.
  // - onremove(ids): a cable's or a module's long press (or right click), then Delete (CableMenu; a module's menu).
  // - onreroute(id, end, next): a cable's long press, then Reroute: its end nearer the press gets a grip, dragged onto
  //   another knob or port (`to`: {module, knob}) or another signal or modulator (`from`: Cable['from']), checked as a
  //   new cable is; let go anywhere else, or Escape, and it stays as it was.
  // - onaddlayer(type), onaddmodule(layerId | 'loop', type), onaddmodulator(type): "+ Add layer", a box's "+",
  //   "+ Add modulator", each with a menu of `addTypes` by place (a source: a layer; a module, into a layer; a loop
  //   module, into the loop; a modulator). A layer that is off ("+ bring back"): onaddlayer(its id), the flame's `flame`.
  // - onpick(request, at): a cable dragged from a knob and let go on nothing: a source for that knob is to be picked
  //   (claude-5f's SourcePicker); `at` where it was let go, in the graph's own coordinates (for a new module's node).
  //
  // Wires and handles by type (0057 §1, patchgraph.ts WIRES); a connection only where its types fit (ports.ts `fits`)
  // and a picture one only where its cycle passes a Feedback (`cyclesClosed`). `live` feeds the control sources' faces.
  import { setContext, untrack } from 'svelte';
  import { SvelteFlow, Panel, type Connection, type Edge, type Node } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/base.css';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import CableMenu from './CableMenu.svelte';
  import PatchNode from './PatchNode.svelte';
  import PatchCable from './PatchCable.svelte';
  import PatchConnection from './PatchConnection.svelte';
  import PatchFit from './PatchFit.svelte';
  import type { Cable, ModuleType, Patch, Place, SignalRef } from './patch';
  import type { PortType } from './ports';
  import type { LiveFaces } from './faces.svelte';
  import {
    CABLE_OPACITIES,
    cableOf,
    knobHandle,
    pictureFits,
    wireStyle,
    type CableData,
    type PickRequest,
    drawnOpacity,
    graphOf,
    GRAPH,
    labelOf,
    nextOpacity,
    rerouted,
    type GraphActions,
    type NodeData,
    type Size,
    type XY,
  } from './patchgraph';

  let {
    patch,
    types,
    positions,
    signals = [],
    addTypes,
    removable = () => true,
    canAddTo = () => true,
    onconnect,
    ondisconnect,
    onknob,
    onmove,
    onopen,
    onopencable,
    onopenknob,
    onremove,
    onreroute,
    onaddlayer,
    onaddmodule,
    onaddmodulator,
    onpick,
    live,
  }: {
    patch: Patch;
    /** Module types by their type id. */
    types: Record<string, ModuleType>;
    positions?: Record<string, XY>;
    /** Signals shown though nothing is cabled from them yet. */
    signals?: SignalRef[];
    /** What can be added (by place); `types` when not given. */
    addTypes?: ModuleType[];
    /** Whether a module can be removed (its long press offers Delete). */
    removable?: (id: string) => boolean;
    /** Whether a box (a layer's id, or `loop`) takes modules from its "+". */
    canAddTo?: (where: string) => boolean;
    onconnect: (from: Cable['from'], to: Cable['to']) => void;
    ondisconnect: (cableId: string) => void;
    onknob: (moduleId: string, knobId: string, value: number) => void;
    onmove: (id: string, x: number, y: number) => void;
    onopen: (id: string) => void;
    onopencable?: (cableId: string) => void;
    onopenknob?: (moduleId: string, knobId: string) => void;
    onremove?: (ids: string[]) => void;
    onreroute?: (cableId: string, end: 'from' | 'to', next: Cable['from'] | Cable['to']) => void;
    onaddlayer?: (type: string) => void;
    onaddmodule?: (where: string, type: string) => void;
    onaddmodulator?: (type: string) => void;
    onpick?: (request: PickRequest, at: XY) => void;
    live?: LiveFaces;
  } = $props();

  const nodeTypes = { patch: PatchNode, box: PatchNode };
  const edgeTypes = { cable: PatchCable };

  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);
  let opened = $state.raw<ReadonlySet<string>>(new Set());
  let moved = $state.raw<Record<string, XY>>({});
  const placed = $derived(positions ?? moved);
  let hovered = $state<string | null>(null);
  let hoveredCable = $state<string | null>(null);
  /** The cable last tapped: its configuration is what the parent shows. */
  let openCable = $state<string | null>(null);
  /** A long press's menu: a cable's, a module's; or the types to add somewhere. */
  type Menu = { kind: 'cable'; id: string; at: XY } | { kind: 'node'; id: string } | { kind: 'add'; to: 'layer' | 'modulator' | { box: string } };
  let menu = $state<Menu | null>(null);
  let reroute = $state<{ id: string; at: XY } | null>(null);
  /** Until the viewer pans, zooms or moves something, the graph keeps all of the patch in view. */
  let touched = $state(false);
  let width = $state(0);
  let height = $state(0);

  // The cables' opacity (VCV Rack's cable opacity), this device's.
  const OPACITY_KEY = 'cratarr-graph-cable-opacity';
  let opacity = $state<number>(
    (() => {
      try {
        const v = Number(localStorage.getItem(OPACITY_KEY));
        return (CABLE_OPACITIES as readonly number[]).includes(v) ? v : 1;
      } catch {
        return 1;
      }
    })(),
  );
  function cycleOpacity() {
    opacity = nextOpacity(opacity);
    try {
      localStorage.setItem(OPACITY_KEY, String(opacity));
    } catch {
      /* this visit only */
    }
  }

  const adds = $derived(!!onaddlayer && !!onaddmodulator);

  // The nodes' measured sizes, as a key, so a measure that changes nothing lays nothing out again.
  const sizeKey = $derived(
    nodes
      .filter((n) => n.type === 'patch' && n.measured?.width)
      .map((n) => `${n.id}=${Math.round(n.measured!.width!)}x${Math.round(n.measured!.height!)}`)
      .join(';'),
  );

  // In focus: the nodes selected or under the pointer (a box, its modules too), and the cables selected, open or under
  // it. Their cables are lit and show their amounts; while anything is in focus, every other cable dims.
  const focusKey = $derived.by(() => {
    const on = new Set(nodes.filter((n) => n.selected).map((n) => n.id));
    if (hovered) on.add(hovered);
    for (const n of nodes) if (n.parentId && on.has(n.parentId)) on.add(n.id);
    const lit = edges.filter((e) => e.selected).map((e) => e.id);
    if (hoveredCable) lit.push(hoveredCable);
    if (openCable) lit.push(openCable);
    return [...on].sort().join('\n') + '\t' + lit.sort().join('\n');
  });
  function dress(list: Edge[], key: string): Edge[] {
    const [a, b] = key.split('\t');
    const on = new Set(a ? a.split('\n') : []);
    const lit = new Set(b ? b.split('\n') : []);
    const any = on.size + lit.size > 0;
    return list.map((e) => {
      if (e.type !== 'cable') return e;
      const shown = lit.has(e.id) || on.has(e.source) || on.has(e.target);
      const d = e.data as CableData | undefined;
      if (d?.normal) return e;
      const muted = d?.muted ? ' muted' : '';
      const cls = (shown ? 'cable lit' : any ? 'cable dim' : 'cable') + muted + ` w-${d?.wire ?? 'value'}`;
      return e.class === cls ? e : { ...e, class: cls, data: { ...e.data, lit: shown } };
    });
  }
  $effect.pre(() => {
    const sizes: Record<string, Size> = {};
    for (const s of sizeKey ? sizeKey.split(';') : []) {
      const [id, wh] = [s.slice(0, s.lastIndexOf('=')), s.slice(s.lastIndexOf('=') + 1)];
      const [width, height] = wh.split('x').map(Number);
      sizes[id] = { width, height };
    }
    const g = graphOf({ patch, types, positions: placed, signals, open: opened, sizes, adds, addTo: (w) => !!onaddmodule && canAddTo(w) });
    untrack(() => {
      // What Svelte Flow knows of each node (its size, selection, a drag under way) carries over.
      const before = new Map(nodes.map((n) => [n.id, n]));
      nodes = g.nodes.map((n) => {
        const b = before.get(n.id);
        return { ...n, measured: b?.measured, selected: b?.selected, ...(b?.dragging ? { position: b.position, dragging: true } : {}) } as Node;
      });
      const chosen = new Set(edges.filter((e) => e.selected).map((e) => e.id));
      edges = dress(
        g.edges.map((e) => ({ ...e, selected: chosen.has(e.id) }) as Edge),
        focusKey,
      );
      // A cable gone (removed, or rerouted to a new id) is no longer open, nor being rerouted.
      const there = new Set(g.edges.map((e) => e.id));
      if (openCable && !there.has(openCable)) openCable = null;
      if (reroute && !there.has(reroute.id)) reroute = null;
    });
  });

  $effect(() => {
    const key = focusKey;
    untrack(() => (edges = dress(edges, key)));
  });

  // What the view fits to: the top-level nodes' places and sizes, and the graph's own size.
  const layout = $derived(
    `${width}x${height};${sizeKey};` +
      nodes
        .filter((n) => !n.parentId)
        .map((n) => `${n.id}@${Math.round(n.position.x)},${Math.round(n.position.y)}`)
        .join(';'),
  );

  // Once a long press has opened a menu, the finger (or button) still down neither pans the view nor drags a node:
  // its moves stop here until it lifts.
  function holdStill() {
    const stop = (e: Event) => e.stopPropagation();
    const done = () => {
      removeEventListener('mousemove', stop, true);
      removeEventListener('touchmove', stop, true);
      removeEventListener('pointerup', done, true);
      removeEventListener('pointercancel', done, true);
    };
    addEventListener('mousemove', stop, true);
    addEventListener('touchmove', stop, true);
    addEventListener('pointerup', done, true);
    addEventListener('pointercancel', done, true);
  }

  function openIt(id: string) {
    openCable = id;
    onopencable?.(id);
  }

  setContext<GraphActions>(GRAPH, {
    knob: (module, knob, value) => onknob(module, knob, value),
    open: (id) => onopen(id),
    toggle(id) {
      const next = new Set(opened);
      if (!next.delete(id)) next.add(id);
      opened = next;
      touched = true;
    },
    opened: (id) => opened.has(id),
    openKnob: (module, knob) => onopenknob?.(module, knob),
    removable: (id) => !!onremove && removable(id) && nodes.some((n) => n.id === id && (n.data as NodeData).kind === 'module'),
    menu(id) {
      holdStill();
      menu = { kind: 'node', id };
    },
    adding: (to) => (menu = { kind: 'add', to }),
    bringBack: (layer) => onaddlayer?.(layer),
    openCable: openIt,
    cableMenu(id, at) {
      if (!onremove && !onreroute) return;
      holdStill();
      reroute = null;
      menu = { kind: 'cable', id, at };
    },
    rerouting: () => reroute,
    live: () => live?.frame ?? null,
  });

  const ends = () => nodes as unknown as { id: string; data: NodeData }[];
  const valid = (c: Connection | Edge) => (c.sourceHandle === 'out' ? pictureFits(c, ends(), edges) : cableOf(c, ends(), patch.cables) !== null);
  // A cable dragged from a knob and let go on nothing: a source for the knob to be picked.
  function connectEnd(_: MouseEvent | TouchEvent, state: { fromHandle?: { id?: string | null; nodeId: string; type: string } | null; toHandle?: unknown; to?: XY | null }) {
    const h = state.fromHandle;
    if (!onpick || !h || state.toHandle || h.type !== 'target' || !h.id?.startsWith('k:') || !state.to) return;
    const d = nodes.find((n) => n.id === h.nodeId)?.data as NodeData | undefined;
    if (d?.kind !== 'module' || !d.knobs.some((k) => knobHandle(k.id) === h.id)) return;
    onpick({ from: 'knob', module: d.module, knob: h.id.slice(2) }, { x: Math.round(state.to.x), y: Math.round(state.to.y) });
  }
  // The wire types drawn, for the legend.
  const LEGEND: [PortType, string][] = [
    ['picture', 'Picture'],
    ['light', 'Light'],
    ['value', 'Value'],
    ['trigger', 'Trigger'],
    ['audio', 'Sound'],
  ];
  const wires = $derived(new Set(edges.map((e) => (e.data as CableData | undefined)?.wire ?? (String(e.class).startsWith('light') ? 'light' : 'picture'))));
  function connect(c: Connection) {
    const cable = cableOf(c, ends(), patch.cables);
    if (cable) onconnect(cable.from, cable.to);
    return null;
  }
  // A rerouted end let go on a knob, a port or an output: to the parent, which hands the patch back changed.
  function reconnect(now: Edge, was: Edge) {
    const cable = patch.cables.find((c) => c.id === was.id);
    const r = cable ? rerouted(cable, now, ends(), patch.cables) : null;
    reroute = null;
    if (r) onreroute?.(was.id, r.end, r.next);
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

  // Names for the menus' heads: a node's, a cable's ("Beat → Kaleidoscope · Turn").
  function nodeName(id: string): string {
    const d = nodes.find((n) => n.id === id)?.data as NodeData | undefined;
    return d && 'label' in d ? d.label : id;
  }
  function cableName(id: string): string {
    const e = edges.find((x) => x.id === id);
    if (!e) return 'Cable';
    const to = nodes.find((n) => n.id === e.target)?.data as NodeData | undefined;
    const knob = e.targetHandle?.slice(2) ?? '';
    const k = to?.kind === 'module' ? (to.knobs.find((x) => x.id === knob)?.label ?? knob[0]?.toUpperCase() + knob.slice(1)) : knob;
    return `${nodeName(e.source)} → ${nodeName(e.target)} · ${k}`;
  }
  // What a "+" offers: the types of its place.
  function offered(to: 'layer' | 'modulator' | { box: string }): ModuleType[] {
    const place: Place = to === 'layer' ? 'source' : to === 'modulator' ? 'modulator' : to.box === 'loop' ? 'loop' : 'module';
    return (addTypes ?? Object.values(types)).filter((t) => t.place === place);
  }
  function add(to: 'layer' | 'modulator' | { box: string }, type: string) {
    menu = null;
    if (to === 'layer') onaddlayer?.(type);
    else if (to === 'modulator') onaddmodulator?.(type);
    else onaddmodule?.(to.box, type);
  }
  const addTitle = (to: 'layer' | 'modulator' | { box: string }) => (to === 'layer' ? 'Add layer' : to === 'modulator' ? 'Add modulator' : `Add to ${nodeName(to.box === 'loop' ? '@loop' : `layer:${to.box}`)}`);
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && reroute && !menu && (reroute = null)} />

<div class="patch-graph" style:--cable-alpha={drawnOpacity(opacity)} bind:clientWidth={width} bind:clientHeight={height}>
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
    connectionLineComponent={PatchConnection}
    isValidConnection={valid}
    onbeforeconnect={connect}
    onconnectend={connectEnd}
    onbeforereconnect={reconnect}
    onreconnectend={() => (reroute = null)}
    onbeforedelete={unplug}
    onnodedragstart={() => (touched = true)}
    onnodedragstop={dropped}
    onmovestart={(event) => {
      if (event) touched = true;
    }}
    onnodepointerenter={({ node }) => (hovered = node.id)}
    onnodepointerleave={() => (hovered = null)}
    onedgepointerenter={({ edge }) => (hoveredCable = edge.id)}
    onedgepointerleave={() => (hoveredCable = null)}
    onedgeclick={({ edge }) => edge.type === 'cable' && openIt(edge.id)}
    onpaneclick={() => (reroute = null)}
  >
    <PatchFit {layout} active={!touched} />
    <Panel position="top-right">
      <div class="legend">
        {#each LEGEND.filter(([t]) => wires.has(t)) as [t, name] (t)}
          <span
            ><svg width="24" height="8" aria-hidden="true"
              ><path d="M1,4 H23" class="w-{t}" style={wireStyle(t)} />{#if t === 'audio'}<path d="M1,4 H23" class="gap" />{/if}</svg
            >{name}</span
          >
        {/each}
        <button class="opacity" aria-label="Cable opacity" title="Cable opacity" onclick={cycleOpacity}>{Math.round(opacity * 100)}%</button>
      </div>
    </Panel>
  </SvelteFlow>
</div>

{#if menu?.kind === 'cable'}
  {@const id = menu.id}
  {@const at = menu.at}
  <CableMenu
    title={cableName(id)}
    onremove={() => onremove?.([id])}
    onreroute={() => {
      openCable = null;
      reroute = { id, at };
    }}
    onclose={() => (menu = null)}
  />
{:else if menu?.kind === 'node'}
  {@const id = menu.id}
  <MenuSheet label={nodeName(id)} title={nodeName(id)} onclose={() => (menu = null)} over>
    <div role="menu">
      <MenuEntry
        icon="trash"
        label="Delete"
        danger
        onclick={() => {
          menu = null;
          onremove?.([id]);
        }}
      />
    </div>
  </MenuSheet>
{:else if menu?.kind === 'add'}
  {@const to = menu.to}
  <MenuSheet label={addTitle(to)} title={addTitle(to)} onclose={() => (menu = null)} over>
    <div role="menu">
      {#each offered(to) as t (t.type)}<MenuEntry label={labelOf(t)} onclick={() => add(to, t.type)} />{/each}
    </div>
  </MenuSheet>
{/if}

<style>
  .patch-graph {
    --patch-mod: #b48cff;
    /* Each wire type's colour (the value's is the modulation purple). */
    --wire-value: var(--patch-mod);
    --wire-trigger: #5fc6ee;
    --wire-audio: #72d39f;
    --wire-light: #f0b04a;
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
  .patch-graph :global(.svelte-flow__edge.light .svelte-flow__edge-path),
  .patch-graph :global(.svelte-flow__edge.feedback .svelte-flow__edge-path) {
    stroke: var(--text);
    stroke-width: var(--wire-w, 2px);
    fill: none;
    opacity: 0.75;
  }
  .patch-graph :global(.svelte-flow__edge.light .svelte-flow__edge-path) {
    stroke: var(--wire-light);
    opacity: 0.6;
  }
  /* A loop's way back: still a picture's solid line, fainter. */
  .patch-graph :global(.svelte-flow__edge.feedback .svelte-flow__edge-path) {
    opacity: 0.35;
  }
  /* The modulation cables at the opacity set (the corner's button); one in focus at all of it. */
  .patch-graph :global(.svelte-flow__edge.cable) {
    opacity: var(--cable-alpha, 1);
    -webkit-touch-callout: none;
  }
  .patch-graph :global(.svelte-flow__edge.cable.lit),
  .patch-graph :global(.svelte-flow__edge.cable.selected) {
    opacity: 1;
  }
  .patch-graph :global(.svelte-flow__edge.cable .svelte-flow__edge-path),
  .patch-graph :global(.svelte-flow__edge.normal .svelte-flow__edge-path) {
    stroke: var(--wire-value);
    stroke-width: var(--wire-w, 1.5px);
    stroke-dasharray: var(--wire-dash, 6 5);
    fill: none;
    opacity: 0.45;
    transition: opacity 0.15s;
  }
  .patch-graph :global(.svelte-flow__edge.w-trigger .svelte-flow__edge-path) {
    stroke: var(--wire-trigger);
    stroke-linecap: round;
  }
  .patch-graph :global(.svelte-flow__edge.w-audio .svelte-flow__edge-path) {
    stroke: var(--wire-audio);
  }
  .patch-graph :global(.svelte-flow__edge.w-light .svelte-flow__edge-path) {
    stroke: var(--wire-light);
  }
  /* A port's normal: what it is wired to until a cable is plugged in; faint, not to be tapped. */
  .patch-graph :global(.svelte-flow__edge.normal) {
    pointer-events: none;
    opacity: 0.45;
  }
  .patch-graph :global(.svelte-flow__edge.normal .svelte-flow__edge-path) {
    opacity: 0.7;
  }
  .patch-graph :global(.svelte-flow__edge.cable.lit .svelte-flow__edge-path) {
    stroke-width: calc(var(--wire-w, 1.5px) + 0.5px);
    opacity: 1;
  }
  .patch-graph :global(.svelte-flow__edge.cable.dim .svelte-flow__edge-path) {
    opacity: 0.1;
  }
  .patch-graph :global(.svelte-flow__edge.cable.selected .svelte-flow__edge-path) {
    stroke-width: calc(var(--wire-w, 1.5px) + 1.5px);
    opacity: 1;
  }
  /* Muted: grey, a sparser dash (its style's). */
  .patch-graph :global(.svelte-flow__edge.cable.muted .svelte-flow__edge-path) {
    stroke: var(--muted);
  }
  /* Its end lifted (rerouting): the cable as it was, faint, under the one being drawn. */
  .patch-graph :global(.svelte-flow__edge.cable .svelte-flow__edge-path.held) {
    opacity: 0.15;
  }
  .patch-graph :global(.svelte-flow__edge-label:not(.svelte-flow__edgeupdater)) {
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
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .legend path {
    fill: none;
    stroke: var(--text);
    stroke-width: var(--wire-w);
    stroke-dasharray: var(--wire-dash);
  }
  .legend path.w-value {
    stroke: var(--wire-value);
  }
  .legend path.w-trigger {
    stroke: var(--wire-trigger);
    stroke-linecap: round;
  }
  .legend path.w-audio {
    stroke: var(--wire-audio);
  }
  .legend path.w-light {
    stroke: var(--wire-light);
  }
  .legend path.gap {
    stroke: var(--patch-bg);
    stroke-width: 1.6px;
    stroke-dasharray: none;
  }
  .opacity {
    min-width: 44px;
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: none;
    color: var(--muted);
    font: inherit;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  .opacity:hover {
    color: var(--text);
  }
</style>
