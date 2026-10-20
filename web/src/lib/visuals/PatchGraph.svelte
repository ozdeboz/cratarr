<script lang="ts">
  // The graph builder (decisions 0056 step 3, 0057 §6, §7): the v2 patch drawn with Svelte Flow, its modules by their
  // categories' lanes (ELK's layout), its cables between typed ports (patchgraph.ts WIRES): pictures and light solid,
  // in steps; values, triggers, sound, points and flow sagging as VCV Rack's. It never changes `patch`: every edit is
  // made on a copy and goes out whole through `onchange(next, key)`, which is the store's (patch-store.ts) `change`
  // (`key` coalesces a drag: a knob being turned is one step). Edits made in one go (a polyphonic knob's channels) are
  // one change.
  //
  // - Edits: a knob turned, a cable drawn (only where its types fit, ports.ts `fits`, and a picture one only where its
  //   cycle passes a Feedback), a cable unplugged (Delete, or its long press's menu), a cable's end rerouted (its long
  //   press, Reroute: the end nearer the press gets a grip), a module added (a lane's "+"), a module deleted (its long
  //   press), modules made a device (Select, or Shift and drag: Make device, its macros the knobs chosen), a device
  //   unmade, a whole category pinned.
  // - `at` (bindable): the devices entered, from the top; a device opened in place is a frame round its modules.
  // - `positions`/`onmove(key, x, y)`: where the viewer put nodes, kept by the parent (the patch keeps none), keyed
  //   `<level>|<node>` (the level the devices entered, joined by `/`; a node's id its path from that level); without
  //   `positions` the graph keeps them itself while it is open. `onarrange`: "Arrange", forgetting them.
  // - onopen(module), onopenknob(module, knob), onopencable(cable): a module's title tapped, a knob tapped (not turned),
  //   a cable (or its amount) tapped; each by its path from the top (a cable's last part its id).
  // - onpick(request, at): a cable dragged from a knob and let go on nothing: a source for that knob is to be picked;
  //   `at` where it was let go, in the graph's own coordinates.
  // - `live` feeds the control sources' faces; `helpFor(type, port?)` a module's help, in its long press's menu.
  //
  // One finger, any screen (0057 §7b): Svelte Flow's own pinch, pan, touch connections (a wider connection radius,
  // a tap on one handle then another) and reconnection; a node moves only past a few pixels, so a long press stays one;
  // labels never under 11 px (they grow as the view zooms out), and far out a node shows its name and how many of its
  // cables come in and go out; jump chips to the picture's parts; `sheetHeight` (an open sheet over the bottom) keeps
  // the fitted view clear of it.
  import { setContext, untrack } from 'svelte';
  import { SvelteFlow, MiniMap, Panel, type Connection, type Edge, type FitViewOptions, type Node } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/base.css';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import CableMenu from './CableMenu.svelte';
  import PatchNode from './PatchNode.svelte';
  import PatchCable from './PatchCable.svelte';
  import PatchWire from './PatchWire.svelte';
  import ElkApi from 'elkjs/lib/elk-api.js';
  import elkWorker from 'elkjs/lib/elk-worker.min.js?url';
  import type { ElkNode } from 'elkjs/lib/elk-api';
  import { fromElk, layoutKey, toElk } from './elk-layout';
  import PatchConnection from './PatchConnection.svelte';
  import PatchFit from './PatchFit.svelte';
  import PatchChips from './PatchChips.svelte';
  import KnobLens from './KnobLens.svelte';
  import type { ModuleType } from './patch';
  import type { PatchV2, PortType } from './ports';
  import type { LiveFaces } from './faces.svelte';
  import { CATEGORY_LABEL } from './rack';
  import {
    applyV2,
    CABLE_OPACITIES,
    connectionV2,
    crumbsOf,
    drawnOpacity,
    EDGE_Z,
    editKey,
    farEdges,
    graphOfV2,
    GRAPH,
    knobHandle,
    labelOf,
    nextOpacity,
    patchAt,
    rerouteV2,
    sizeOf,
    typesOf,
    wireStyle,
    type AddTo,
    type CableData,
    type Fine,
    type GraphActions,
    type NodeData,
    type PickRequest,
    type Size,
    type V2Edit,
    type XY,
  } from './patchgraph';

  let {
    patch,
    onchange,
    types,
    at = $bindable([]),
    positions,
    onmove,
    onopen,
    onopencable,
    onopenknob,
    onpick,
    live,
    openAtStart = [],
    sheetHeight = 0,
    layout = 'elk',
    routeCables = false,
    onarrange,
    helpFor,
    onedges,
  }: {
    /** The patch (the store's, unflattened). */
    patch: PatchV2;
    /** Each edit: the patch it makes (a new object; `patch` is never changed), and what it changes (`knob:<path>.<knob>`
     *  while a knob turns), for the store to coalesce. */
    onchange: (next: PatchV2, key?: string) => void;
    /** Module types by their type id. */
    types: Record<string, ModuleType>;
    /** The devices entered, from the top. */
    at?: string[];
    /** Where the viewer put nodes, keyed `<level>|<node>`. */
    positions?: Record<string, XY>;
    onmove?: (key: string, x: number, y: number) => void;
    onopen?: (module: string[]) => void;
    onopencable?: (cable: string[]) => void;
    onopenknob?: (module: string[], knob: string) => void;
    onpick?: (request: PickRequest, at: XY) => void;
    live?: LiveFaces;
    /** Nodes (devices) open from the start. */
    openAtStart?: string[];
    /** How much of the graph's bottom an open sheet covers (px): the fitted view keeps clear of it. */
    sheetHeight?: number;
    /** The layout: ELK's (layered, in a worker; the owner's choice) or the hand one (the lab's comparison). */
    layout?: 'elk' | 'hand';
    /** Whether modulation cables follow ELK's routes too (else they sag, as VCV Rack's). */
    routeCables?: boolean;
    /** "Arrange": the places the viewer gave nodes forgotten, ELK's taken (shown while there are any). */
    onarrange?: () => void;
    /** A module's help (module-help.ts), for its long press's menu. */
    helpFor?: (type: string, port?: string) => string | null | undefined;
    /** The edges as drawn, each time they change (the lab's check that every cable ends on its sockets). */
    onedges?: (edges: { id: string; source: string; target: string; sourceHandle?: string | null; targetHandle?: string | null; wire: string }[]) => void;
  } = $props();

  const coarse = typeof matchMedia === 'function' && matchMedia('(pointer: coarse)').matches;
  // The top panels clear of the safe area (a notch).
  const TOP = 'margin-top: calc(15px + env(safe-area-inset-top))';
  // The view's zoom: no text under 11 px on screen. Under 1 the small labels (11 px) are not drawn; under FAR a title
  // (13.5 px) would be under 11 too, and nodes show their name and how much is cabled.
  const FAR = 0.82;
  let zoom = $state(1);
  // The overview map on a narrow screen: shut until asked for.
  let mapOpen = $state(false);
  // A knob's fine adjust under way (its lens over the graph), and its help.
  let fine = $state.raw<Fine | null>(null);
  let fineEnded = -Infinity;
  const fineHelp = $derived.by(() => {
    if (!fine || !helpFor) return null;
    const d = nodes.find((n) => n.id === fine!.node)?.data as NodeData | undefined;
    return d?.kind === 'module' ? (helpFor(d.type, fine.knob) ?? null) : null;
  });
  const farNow = $derived(zoom < FAR);
  // What the view fits round: clear of the legend and crumbs above, the chips and an open sheet below.
  const pad = $derived({ top: 56, left: 16, right: 16, bottom: Math.round(sheetHeight) + 60 });
  const padding = $derived<FitViewOptions['padding']>({ top: `${pad.top}px`, left: `${pad.left}px`, right: `${pad.right}px`, bottom: `${pad.bottom}px` });
  // The overview map's nodes by class (themed below): a modulator's purple, the rest the tiles' grey, frames left out.
  const mapClass = (n: Node) => {
    const d = n.data as NodeData;
    if (d.kind !== 'module') return 'map-none';
    return d.modulator ? 'map-mod' : 'map-node';
  };

  const nodeTypes = { patch: PatchNode, box: PatchNode };
  const edgeTypes = { cable: PatchCable, wire: PatchWire };

  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);
  // The picture's parts, for the chips: a device opened here, a module the picture runs through.
  const parts = $derived(
    nodes
      .filter((n) => !n.parentId)
      .flatMap((n) => {
        const d = n.data as NodeData;
        if (d.kind === 'box') return [{ id: n.id, label: d.label }];
        if (d.kind === 'module' && (d.in || d.out) && !d.modulator) return [{ id: n.id, label: d.label }];
        return [];
      }),
  );
  let opened = $state.raw<ReadonlySet<string>>(new Set(untrack(() => openAtStart)));

  // The level shown: the devices entered (none where `at` no longer leads anywhere), and its patch.
  const top = $derived(patchAt(patch, at));
  $effect(() => {
    if (!top && at.length) at = [];
  });
  const levelKey = $derived(at.join('/'));
  const path = (id: string) => [...at, ...id.split('/')];
  // Where nodes were put, at this level: by node id.
  let moved = $state.raw<Record<string, XY>>({});
  const kept = $derived(positions ?? moved);
  const placed = $derived(Object.fromEntries(Object.entries(kept).flatMap(([k, v]) => (k.startsWith(`${levelKey}|`) ? [[k.slice(levelKey.length + 1), v]] : []))));

  let hovered = $state<string | null>(null);
  let hoveredCable = $state<string | null>(null);
  /** The cable last tapped: its configuration is what the parent shows. */
  let openCable = $state<string | null>(null);
  /** A long press's menu: a cable's, a module's, a knob's or port's help; the types to add somewhere; Make device. */
  type Menu = { kind: 'cable'; id: string; at: XY } | { kind: 'node'; id: string } | { kind: 'add'; to: AddTo } | { kind: 'make'; ids: string[] } | { kind: 'help'; id: string; port: string };
  // The knobs chosen as a device's macros while it is being made (`<node>|<knob>`).
  let picks = $state<string[]>([]);
  // Selecting (a module's long press, Select): taps add or take away modules, for Make device; a finger's way to what
  // Shift and a drag do.
  let selecting = $state(false);
  let sel = $state<string[]>([]);
  function togglePick(id: string) {
    const d = nodes.find((n) => n.id === id)?.data as NodeData | undefined;
    if (d?.kind !== 'module') return;
    sel = sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id];
  }
  $effect(() => {
    const on = selecting ? new Set(sel) : null;
    untrack(() => {
      if (on) nodes = nodes.map((n) => (!!n.selected === on.has(n.id) ? n : { ...n, selected: on.has(n.id) }));
    });
  });
  let menu = $state<Menu | null>(null);
  let reroute = $state<{ id: string; at: XY } | null>(null);
  /** Until the viewer pans, zooms or moves something, the graph keeps all of the patch in view. */
  let touched = $state(false);
  let width = $state(0);
  let height = $state(0);
  // Taller than wide where the graph shows (above an open sheet): the lanes run down (a phone upright); the view the
  // layout fits (less the fit's padding).
  const portrait = $derived(width > 0 && height - sheetHeight > width);
  const view = $derived(portrait ? { width: Math.max(1, width - 32), height: Math.max(1, height - sheetHeight - 116) } : undefined);

  // ---- Edits: each on a copy of the patch, out whole -------------------------------------------------------------
  // Edits in one go (before the parent hands the patch back) build on each other.
  let ahead: { from: PatchV2; next: PatchV2 } | null = null;
  function edit(list: V2Edit[], key: string | null = list.length === 1 ? editKey(list[0]) : null) {
    if (!list.length) return;
    const base = ahead && ahead.from === patch ? ahead.next : patch;
    let next = $state.snapshot(base) as PatchV2;
    for (const e of list) next = applyV2(next, e, types);
    const from = patch;
    ahead = { from, next };
    queueMicrotask(() => {
      if (ahead?.from === from) ahead = null;
    });
    onchange(next, key ?? undefined);
  }

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

  // ELK in a web worker (elk-layout.ts), made when first wanted; its last layout by the graph's shape.
  let elk: InstanceType<typeof ElkApi> | null = null;
  let elkDone = $state.raw<{ key: string; laid: ElkNode } | null>(null);
  let elkAsked: string | null = null;
  function askElk(key: string, graph: ElkNode) {
    if (elkAsked === key) return;
    elkAsked = key;
    elk ??= new ElkApi({ workerUrl: elkWorker });
    elk
      .layout(graph)
      .then((laid) => {
        if (elkAsked === key) elkDone = { key, laid: laid as ElkNode };
      })
      .catch((e) => console.warn('Visuals: the graph could not be laid out', e));
  }
  $effect(() => () => elk?.terminateWorker());
  // Into another level: the view fits it again.
  $effect(() => {
    void levelKey;
    untrack(() => (touched = false));
  });

  // The nodes' measured sizes, as a key, so a measure that changes nothing lays nothing out again.
  const sizeKey = $derived(
    nodes
      .filter((n) => n.type === 'patch' && n.measured?.width)
      .map((n) => `${n.id}=${Math.round(n.measured!.width!)}x${Math.round(n.measured!.height!)}`)
      .join(';'),
  );

  // In focus: the nodes selected or under the pointer (a frame, its modules too), and the cables selected, open or
  // under it. Their cables are lit and show their amounts; while anything is in focus, every other cable dims.
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
    const g = graphOfV2({ patch, types, at, open: opened, positions: placed, sizes, portrait, view });
    // By ELK: its layout once it is in for this graph's shape (a knob's turn does not change it), else asked for.
    if (layout === 'elk') {
      const sz = (n: Node | (typeof g.nodes)[number]) => sizes[n.id] ?? sizeOf((n as (typeof g.nodes)[number]).data);
      const key = layoutKey(g.nodes, g.edges, sz, { portrait });
      if (elkDone?.key === key) Object.assign(g, fromElk(elkDone.laid, g.nodes, g.edges, (id) => !!placed[id], sz));
      else untrack(() => askElk(key, toElk(g.nodes, g.edges, sz, { portrait })));
    }
    // Far out, every cable on its nodes' far sockets (none ends inside a node); every cable over the nodes (EDGE_Z).
    if (farNow) g.edges = farEdges(g.edges, g.nodes);
    g.edges = g.edges.map((e) => ({ ...e, zIndex: EDGE_Z }));
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
      // A cable gone (removed, or at another level) is no longer open, nor being rerouted.
      const there = new Set(g.edges.map((e) => e.id));
      if (openCable && !there.has(openCable)) openCable = null;
      if (reroute && !there.has(reroute.id)) reroute = null;
    });
  });

  $effect(() => {
    const key = focusKey;
    untrack(() => (edges = dress(edges, key)));
  });
  $effect(() =>
    onedges?.(
      edges.map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        sourceHandle: e.sourceHandle,
        targetHandle: e.targetHandle,
        wire: (e.data as CableData | undefined)?.wire ?? 'picture',
      })),
    ),
  );

  // What the view fits to: the top-level nodes' places and sizes, and the graph's own size.
  const fitKey = $derived(
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
    onopencable?.(path(id));
  }
  const dataOf = (id: string) => nodes.find((n) => n.id === id)?.data as NodeData | undefined;
  /** A cable's edit scope and id from its edge's id (the devices it is inside, from the level shown). */
  function cableAt(id: string): { scope: string[]; cable: string } {
    const cut = id.lastIndexOf('/');
    return { scope: [...at, ...(cut < 0 ? [] : id.slice(0, cut).split('/'))], cable: cut < 0 ? id : id.slice(cut + 1) };
  }
  const enter = (id: string) => (at = path(id));

  setContext<GraphActions>(GRAPH, {
    knob: (module, knob, value) => edit([{ kind: 'knob', module: path(module), knob, value }]),
    knobs: (module, poly, values) =>
      edit(
        values.map((v) => ({ kind: 'knob', module: path(module), knob: v.id, value: v.value })),
        `knob:${path(module).join('/')}.${poly}`,
      ),
    open(id) {
      if (selecting) return togglePick(id);
      const d = dataOf(id);
      // A device's title (or its frame): into it; a module's, its page.
      if (d?.kind === 'box' || (d?.kind === 'module' && d.device)) enter(id);
      else if (d?.kind === 'module') onopen?.(path(id));
    },
    enter,
    fine(f) {
      if (!f && fine) fineEnded = performance.now();
      fine = f;
    },
    help(id, port) {
      const d = dataOf(id);
      if (d?.kind !== 'module' || !helpFor?.(d.type, port)) return;
      holdStill();
      menu = { kind: 'help', id, port };
    },
    toggle(id) {
      const next = new Set(opened);
      if (!next.delete(id)) next.add(id);
      opened = next;
      touched = true;
    },
    opened: (id) => opened.has(id),
    openKnob: (module, knob) => onopenknob?.(path(module), knob),
    menu(id) {
      // Not while a knob is being fine-adjusted, nor just after (a touch's long press sends its context menu to whatever
      // is under the finger by then).
      if (fine || performance.now() - fineEnded < 600 || dataOf(id)?.kind !== 'module') return;
      holdStill();
      menu = { kind: 'node', id };
    },
    adding: (to) => (menu = { kind: 'add', to }),
    pinCategory: (scope, category, on) => edit([{ kind: 'pin', scope: [...at, ...scope], category, on }]),
    openCable: openIt,
    cableMenu(id, where) {
      holdStill();
      reroute = null;
      menu = { kind: 'cable', id, at: where };
    },
    rerouting: () => reroute,
    live: () => live?.frame ?? null,
    portrait: () => portrait,
    routeCables: () => routeCables,
  });

  const ends = () => nodes as unknown as { id: string; data: NodeData }[];
  // A connection where it may go (a cable being rerouted left out of the checks).
  const valid = (c: Connection | Edge) => !!top && connectionV2(c, ends(), top, at, types, reroute ? cableAt(reroute.id).cable : undefined) !== null;
  // A cable dragged from a knob and let go on nothing: a source for the knob to be picked.
  function connectEnd(_: MouseEvent | TouchEvent, state: { fromHandle?: { id?: string | null; nodeId: string; type: string } | null; toHandle?: unknown; to?: XY | null }) {
    const h = state.fromHandle;
    if (!onpick || !h || state.toHandle || h.type !== 'target' || !h.id?.startsWith('k:') || !state.to) return;
    const d = dataOf(h.nodeId);
    if (d?.kind !== 'module' || !d.knobs.some((k) => knobHandle(k.id) === h.id)) return;
    onpick({ from: 'knob', module: path(h.nodeId), knob: h.id.slice(2) }, { x: Math.round(state.to.x), y: Math.round(state.to.y) });
  }
  // Modules selected at one level: what "Make device" makes a device of.
  const chosen = $derived.by(() => {
    const ids = nodes.filter((n) => n.selected && (n.data as NodeData).kind === 'module').map((n) => n.id);
    const scopes = new Set(ids.map((id) => id.split('/').slice(0, -1).join('/')));
    return scopes.size === 1 ? ids : [];
  });
  function make(ids: string[]) {
    const scope = ids[0].split('/').slice(0, -1);
    const macros = picks.map((p) => {
      const [node, knob] = p.split('|');
      return { module: node.split('/').pop()!, knob };
    });
    edit([{ kind: 'make', scope: [...at, ...scope], modules: ids.map((id) => id.split('/').pop()!), macros }]);
    menu = null;
    picks = [];
    selecting = false;
    sel = [];
  }
  const knobsOfNode = (id: string) => {
    const d = dataOf(id);
    if (d?.kind !== 'module') return [];
    return (types[d.type]?.knobs ?? []).filter((k) => !k.device).map((k) => ({ key: `${id}|${k.id}`, label: `${d.label} · ${k.label}` }));
  };
  // The wire types drawn, for the legend.
  const LEGEND: [PortType, string][] = [
    ['picture', 'Picture'],
    ['light', 'Light'],
    ['value', 'Value'],
    ['trigger', 'Trigger'],
    ['audio', 'Sound'],
    ['points', 'Points'],
    ['flow', 'Flow'],
  ];
  const wires = $derived(new Set(edges.map((e) => (e.data as CableData | undefined)?.wire ?? 'picture')));
  function connect(c: Connection) {
    const made = top ? connectionV2(c, ends(), top, at, types) : null;
    if (made) edit([made]);
    return null;
  }
  // A rerouted end let go on another socket: the cable moved there, where it may go.
  function reconnect(now: Edge, was: Edge) {
    const r = top ? rerouteV2(was.id, now, ends(), top, at, types) : null;
    reroute = null;
    if (r) edit([r]);
    return null;
  }
  async function unplug({ edges: gone }: { nodes: Node[]; edges: Edge[] }) {
    // Cables at their levels, one change.
    const by = new Map<string, { scope: string[]; cables: string[] }>();
    for (const e of gone) {
      if (e.deletable === false) continue;
      const { scope, cable } = cableAt(e.id);
      const k = scope.join('/');
      (by.get(k) ?? by.set(k, { scope, cables: [] }).get(k)!).cables.push(cable);
    }
    edit([...by.values()].map((x) => ({ kind: 'remove' as const, ...x })));
    return false;
  }
  function removeCable(id: string) {
    const { scope, cable } = cableAt(id);
    edit([{ kind: 'remove', scope, cables: [cable] }]);
  }
  function removeModule(id: string) {
    const scope = id.split('/').slice(0, -1);
    edit([{ kind: 'removeModules', scope: [...at, ...scope], modules: [id.split('/').pop()!] }]);
  }
  function dropped({ nodes: released }: { nodes: Node[] }) {
    const next = { ...moved };
    for (const n of released) {
      const key = `${levelKey}|${n.id}`;
      next[key] = { x: Math.round(n.position.x), y: Math.round(n.position.y) };
      onmove?.(key, next[key].x, next[key].y);
    }
    if (!positions) moved = next;
  }

  // Names for the menus' heads: a node's, a cable's ("Beat → Kaleidoscope · Turn").
  function nodeName(id: string): string {
    const d = dataOf(id);
    return d && 'label' in d ? d.label : id;
  }
  function cableName(id: string): string {
    const e = edges.find((x) => x.id === id);
    if (!e) return 'Cable';
    const to = dataOf(e.target);
    const port = e.targetHandle === 'in' ? (to?.kind === 'module' ? (to.v2.pin ?? '') : '') : (e.targetHandle?.slice(2) ?? '');
    const k = to?.kind === 'module' ? (to.knobs.find((x) => x.id === port)?.label ?? to.v2.ports.in.find((x) => x.id === port)?.label ?? port) : port;
    return `${nodeName(e.source)} → ${nodeName(e.target)} · ${k}`;
  }
  const addTitle = (to: AddTo) => `Add · ${CATEGORY_LABEL[to.category]}`;
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && reroute && !menu && (reroute = null)} />

<div class="patch-graph" style:--graph-w="{width}px" class:small={zoom < 1} class:far={zoom < FAR} class:narrow={width < 600} style:--cable-alpha={drawnOpacity(opacity)} style:--z={zoom} bind:clientWidth={width} bind:clientHeight={height}>
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
    {edgeTypes}
    colorMode="dark"
    minZoom={0.2}
    maxZoom={2}
    zoomOnDoubleClick={false}
    connectionRadius={coarse ? 36 : 20}
    clickConnect
    nodeDragThreshold={coarse ? 10 : 1}
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
    onedgeclick={({ edge }) => edge.type === 'cable' && !selecting && openIt(edge.id)}
    onnodeclick={({ node }) => selecting && togglePick(node.id)}
    onpaneclick={() => (reroute = null)}
  >
    <PatchFit layout={fitKey} active={!touched} {pad} view={{ width, height }} {portrait} onzoom={(z) => (zoom = z)} />
    <!-- The map: bottom right over the chips; on a narrow screen a button (Map) opens it top right, over the view only
         while wanted. -->
    {#if width >= 600 || mapOpen}
    <MiniMap
      position={width < 600 ? 'top-right' : 'bottom-right'}
      pannable
      zoomable
      width={coarse ? 112 : 168}
      height={coarse ? 84 : 112}
      nodeClass={mapClass}
      ariaLabel="Overview"
      style={`${width < 600 ? 'margin: calc(12px + env(safe-area-inset-top)) 12px 0 0' : `margin: 0 16px calc(${Math.round(sheetHeight)}px + 64px + env(safe-area-inset-bottom)) 0`}; border: 1px solid var(--border); border-radius: 10px; overflow: hidden`}
    />
    {/if}
    <Panel position="bottom-left" class="chips-panel">
      <div class="bottom">
        {#if selecting}
          <div class="selbar" role="toolbar" aria-label="Selecting">
            <span>{sel.length}</span>
            <button class="opacity make" disabled={chosen.length < 2} onclick={() => (menu = { kind: 'make', ids: chosen })}>Make device</button>
            <button class="opacity" onclick={() => ((selecting = false), (sel = []))}>Cancel</button>
          </div>
        {:else}
          {#if chosen.length > 1}<button class="opacity make" onclick={() => (menu = { kind: 'make', ids: chosen })}>Make device</button>{/if}
          <PatchChips {parts} {padding} onjump={() => (touched = true)} />
        {/if}
        {#if onarrange && Object.keys(kept).length}<button class="opacity" onclick={onarrange}>Arrange</button>{/if}
        {#if width < 600}<button class="opacity" class:on={mapOpen} aria-pressed={mapOpen} onclick={() => (mapOpen = !mapOpen)}>Map</button>{/if}
        <button class="opacity" aria-label="Cable opacity" title="Cable opacity" onclick={cycleOpacity}>{Math.round(opacity * 100)}%</button>
      </div>
    </Panel>
    {#if at.length}
      <Panel position="top-left" style={TOP}>
        <nav class="crumbs" aria-label="Devices">
          <button onclick={() => (at = [])}>Rack</button>
          {#each crumbsOf(patch, at) as name, i (i)}<span>›</span><button class:here={i === at.length - 1} onclick={() => (at = at.slice(0, i + 1))}>{name}</button>{/each}
        </nav>
      </Panel>
    {/if}
    <Panel position="top-right" style={TOP}>
      <div class="legend">
        {#each LEGEND.filter(([t]) => wires.has(t)) as [t, name] (t)}
          <span
            ><svg width="24" height="8" aria-hidden="true"
              ><path d="M1,4 H23" class="w-{t}" style={wireStyle(t)} />{#if t === 'audio' || t === 'flow'}<path d="M1,4 H23" class="gap" />{/if}</svg
            >{name}</span
          >
        {/each}
      </div>
    </Panel>
  </SvelteFlow>
</div>

{#if fine}<KnobLens {fine} help={fineHelp} />{/if}

{#if menu?.kind === 'cable'}
  {@const id = menu.id}
  {@const where = menu.at}
  <CableMenu
    title={cableName(id)}
    onremove={() => removeCable(id)}
    onreroute={() => {
      openCable = null;
      reroute = { id, at: where };
    }}
    onclose={() => (menu = null)}
  />
{:else if menu?.kind === 'node'}
  {@const id = menu.id}
  {@const d = nodes.find((n) => n.id === id)?.data as NodeData | undefined}
  {@const help = d?.kind === 'module' ? helpFor?.(d.type) : null}
  <MenuSheet label={nodeName(id)} title={nodeName(id)} onclose={() => (menu = null)} over>
    {#if help}<p class="help">{help}</p>{/if}
    <div role="menu">
      {#if d?.kind === 'module'}
        <MenuEntry
          label="Select"
          onclick={() => {
            menu = null;
            selecting = true;
            sel = [id];
          }}
        />
      {/if}
      {#if d?.kind === 'module' && d.device}
        <MenuEntry
          icon="expand"
          label="Open"
          onclick={() => {
            menu = null;
            const next = new Set(opened);
            next.add(id);
            opened = next;
          }}
        />
        <MenuEntry
          label="Unmake"
          onclick={() => {
            menu = null;
            edit([{ kind: 'unmake', scope: [...at, ...id.split('/').slice(0, -1)], device: id.split('/').pop()! }]);
          }}
        />
      {/if}
      {#if d?.kind === 'module'}
        <MenuEntry
          icon="trash"
          label="Delete"
          danger
          onclick={() => {
            menu = null;
            removeModule(id);
          }}
        />
      {/if}
    </div>
  </MenuSheet>
{:else if menu?.kind === 'help'}
  {@const hid = menu.id}
  {@const port = menu.port}
  {@const d = nodes.find((n) => n.id === hid)?.data as NodeData | undefined}
  {#if d?.kind === 'module'}
    {@const name = d.knobs.find((k) => k.id === port)?.label ?? d.ports.find((p) => p.id === port)?.label ?? port}
    <MenuSheet label="{d.label} · {name}" title="{d.label} · {name}" onclose={() => (menu = null)} over>
      <p class="help">{helpFor?.(d.type, port)}</p>
    </MenuSheet>
  {/if}
{:else if menu?.kind === 'make'}
  {@const ids = menu.ids}
  <MenuSheet label="Make device" title="Make device" onclose={() => ((menu = null), (picks = []))} over>
    <div role="menu">
      {#each ids.flatMap(knobsOfNode) as k (k.key)}
        <MenuEntry label={k.label} checked={picks.includes(k.key)} onclick={() => (picks = picks.includes(k.key) ? picks.filter((x) => x !== k.key) : [...picks, k.key])} />
      {/each}
      <MenuEntry icon="intoBox" label="Make" onclick={() => make(ids)} />
    </div>
  </MenuSheet>
{:else if menu?.kind === 'add'}
  {@const to = menu.to}
  <MenuSheet label={addTitle(to)} title={addTitle(to)} onclose={() => (menu = null)} over>
    <div role="menu">
      {#each typesOf(to.category, types) as t (t.type)}<MenuEntry
          label={labelOf(t)}
          onclick={() => {
            menu = null;
            edit([{ kind: 'add', scope: [...at, ...to.scope], type: t.type }]);
          }}
        />{/each}
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
    --wire-points: #e98fc8;
    /* A flow field's: lime, apart from the trigger's cyan and the sound's green. */
    --wire-flow: #c2d94c;
    --patch-bg: color-mix(in srgb, var(--bg) 92%, var(--text));
    width: 100%;
    height: 100%;
    background: var(--patch-bg);
    color: var(--text);
  }
  .patch-graph :global(.svelte-flow) {
    /* The overview map (Svelte Flow's MiniMap, by its theming variables). */
    --xy-minimap-background-color: var(--patch-bg);
    --xy-minimap-mask-background-color: color-mix(in srgb, var(--bg) 55%, transparent);
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
  .patch-graph :global(.svelte-flow__edge.w-points .svelte-flow__edge-path) {
    stroke: var(--wire-points);
    stroke-linecap: round;
  }
  .patch-graph :global(.svelte-flow__edge.w-flow .svelte-flow__edge-path) {
    stroke: var(--wire-flow);
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
  .legend path.w-points {
    stroke: var(--wire-points);
    stroke-linecap: round;
  }
  .legend path.w-flow {
    stroke: var(--wire-flow);
  }
  .legend path.gap {
    stroke: var(--patch-bg);
    stroke-width: 1.6px;
    stroke-dasharray: none;
  }
  .crumbs {
    display: flex;
    gap: 6px;
    align-items: baseline;
    color: var(--muted);
    font-size: 13px;
  }
  .crumbs button {
    padding: 2px 4px;
    border: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .crumbs button:hover,
  .crumbs button.here {
    color: var(--text);
  }
  /* Over the bottom: the chips and the opacity button, above an open sheet and the safe area. */
  .patch-graph :global(.chips-panel) {
    margin: 0 16px calc(var(--sheet-h, 0px) + 12px + env(safe-area-inset-bottom)) calc(16px + env(safe-area-inset-left));
  }
  .bottom {
    display: flex;
    gap: 8px;
    align-items: center;
    max-width: calc(var(--graph-w, 100vw) - 32px - env(safe-area-inset-left) - env(safe-area-inset-right));
  }
  .bottom > :global(.chips) {
    flex: 1 1 auto;
    min-width: 0;
  }
  .selbar {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--muted);
    font-size: 13px;
  }
  .make:disabled {
    opacity: 0.5;
    cursor: default;
  }
  /* On a narrow screen the legend gives way. */
  .narrow .legend {
    display: none;
  }
  /* Labels never under 11 px: each grows as the view zooms out (--z, the zoom). */
  .patch-graph.small :global(.svelte-flow__edge-label:not(.svelte-flow__edgeupdater)) {
    visibility: hidden;
  }
  .help {
    margin: 0 20px 12px;
    color: var(--muted);
    font-size: 14px;
    line-height: 1.45;
  }
  .opacity.on {
    color: var(--text);
    border-color: color-mix(in srgb, var(--text) 35%, var(--border));
  }
  .make {
    color: var(--text);
    white-space: nowrap;
  }
  .patch-graph :global(.map-node) {
    fill: color-mix(in srgb, var(--text) 30%, var(--surface));
  }
  .patch-graph :global(.map-mod) {
    fill: color-mix(in srgb, var(--patch-mod) 45%, var(--surface));
  }
  .patch-graph :global(.map-none) {
    fill: none;
  }
  .opacity {
    min-height: 32px;
    flex: none;
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
