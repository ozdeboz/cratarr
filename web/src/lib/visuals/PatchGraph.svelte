<script lang="ts" module>
  import { panSafe } from '../panSafe';
  import type { ElkNode as LaidNode } from 'elkjs/lib/elk-api';
  /** ELK's layouts this page made, by the graph's shape: the rack opened again is laid at once. The last few. */
  const laidBefore = new Map<string, { laid: LaidNode; portrait: boolean }>();
  function keepLaid(key: string, laid: { laid: LaidNode; portrait: boolean }) {
    laidBefore.delete(key);
    laidBefore.set(key, laid);
    if (laidBefore.size > 24) laidBefore.delete(laidBefore.keys().next().value!);
  }
</script>

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
  import { SvelteFlow, Panel, type Connection, type Edge, type FitViewOptions, type Node } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/base.css';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { longpress } from '../longpress';
  import SelectBar from '../SelectBar.svelte';
  import Icon from '../Icon.svelte';
  import PatchNode from './PatchNode.svelte';
  import PatchCable from './PatchCable.svelte';
  import PatchWire from './PatchWire.svelte';
  import ElkApi from 'elkjs/lib/elk-api.js';
  import elkWorker from 'elkjs/lib/elk-worker.min.js?url';
  import type { ElkNode } from 'elkjs/lib/elk-api';
  import { anchored, fromElk, layoutKey, toElk } from './elk-layout';
  import PatchConnection from './PatchConnection.svelte';
  import PatchFit from './PatchFit.svelte';
  import PatchChips from './PatchChips.svelte';
  import KnobLens from './KnobLens.svelte';
  import type { ModuleType } from './patch';
  import type { PatchV2, PortType } from './ports';
  import type { LiveFaces } from './faces.svelte';
  import { CATEGORIES, CATEGORY_LABEL } from './rack';
  import { previewOf, previewTarget, Thumbs, type Thumbnailer } from './face/face';
  import Preview from './face/Preview.svelte';
  import { MUSIC_SIGNALS, musicModuleFor, SIGNAL_OUT } from './music-signals';
  import { fits } from './ports';
  import { applyEdit } from './patch-edits';
  import {
    CABLE_OPACITY_MIN,
    cableOpacity,
    drawnOpacity,
    categoryPin,
    barActions,
    deleteEdits,
    unmakeEdits,
    freeModuleId,
    connectionV2,
    crumbsOf,
    EDGE_Z,
    editKey,
    graphOfV2,
    GRAPH,
    knobHandle,
    labelOf,
    patchAt,
    rerouteV2,
    sizeOf,
    typesOf,
    wireStyle,
    type AddTo,
    type CableData,
    type Fine,
    type GraphActions,
    type ModuleData,
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
    thumbnail,
    previewShown = false,
    tapOpens = false,
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
    /** Draws a module's output into a thumbnail's canvas (the renderer's `thumbnail(moduleId, canvas)`): one a live
     *  frame, in turn, only while the graph shows (live frames come only then). */
    thumbnail?: Thumbnailer;
    /** The page shows the picked module's preview itself (its sheet's top): the graph's popover then gives way. */
    previewShown?: boolean;
    /** A tap on a module opens it as its title does (the graph alone: step 8, the owner), not only shows its preview. */
    tapOpens?: boolean;
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
  // A knob's fine adjust under way (its lens over the graph), and its help.
  let fine = $state.raw<Fine | null>(null);
  let fineEnded = -Infinity;
  const fineHelp = $derived.by(() => {
    if (!fine || !helpFor) return null;
    const d = nodes.find((n) => n.id === fine!.node)?.data as NodeData | undefined;
    return d?.kind === 'module' ? (helpFor(d.type, fine.knob) ?? null) : null;
  });
  // What the view fits round: clear of the legend and crumbs above, the chips and an open sheet below.
  const pad = $derived({ top: 56, left: 16, right: 16, bottom: Math.round(sheetHeight) + 60 });
  const padding = $derived<FitViewOptions['padding']>({ top: `${pad.top}px`, left: `${pad.left}px`, right: `${pad.right}px`, bottom: `${pad.bottom}px` });

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
  type Menu =
    // The module browser (the graph's one "+"): every type by category, Music's signals, a category's pin.
    | { kind: 'browser' }
    // The graph's view options (the Rack's long press): kept on this device, not in the patch.
    | { kind: 'view' }
    | { kind: 'make'; ids: string[] }
    | { kind: 'help'; id: string; port: string }
    // From a knob dragged into the empty: the music signals that fit it, a module of one cabled into it.
    | { kind: 'pick'; node: string; port: string; at: XY };
  // The knobs chosen as a device's macros while it is being made (`<node>|<knob>`).
  let picks = $state<string[]>([]);
  // Selecting (a module's or a cable's long press, Select): taps add or take away modules, devices and cables, and the
  // select bar pins, unpins, makes a device of them or deletes them at once; a finger's way to what a Shift- or
  // Ctrl-click does on a desktop.
  let selecting = $state(false);
  let sel = $state<string[]>([]);
  let selCables = $state<string[]>([]);
  function togglePick(id: string) {
    const d = nodes.find((n) => n.id === id)?.data as NodeData | undefined;
    if (d?.kind !== 'module' || d.type === 'screen') return;
    sel = sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id];
  }
  function toggleCable(id: string) {
    if (edges.find((e) => e.id === id)?.deletable === false) return;
    selCables = selCables.includes(id) ? selCables.filter((x) => x !== id) : [...selCables, id];
  }
  function endSelect() {
    selecting = false;
    sel = [];
    selCables = [];
    nodes = nodes.map((n) => (n.selected ? { ...n, selected: false } : n));
    edges = edges.map((e) => (e.selected ? { ...e, selected: false } : e));
  }
  $effect(() => {
    const on = selecting ? new Set(sel) : null;
    const lit = selecting ? new Set(selCables) : null;
    untrack(() => {
      if (on) nodes = nodes.map((n) => (!!n.selected === on.has(n.id) ? n : { ...n, selected: on.has(n.id) }));
      if (lit) edges = edges.map((e) => (!!e.selected === lit.has(e.id) ? e : { ...e, selected: lit.has(e.id) }));
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
    for (const e of list) next = applyEdit(next, e, types);
    const from = patch;
    ahead = { from, next };
    queueMicrotask(() => {
      if (ahead?.from === from) ahead = null;
    });
    onchange(next, key ?? undefined);
  }

  /** A node's sockets, as a key: what its cables' ends need to find. */
  const socketsOf = (d: NodeData) =>
    d.kind !== 'module' ? d.kind : [d.in, d.out, ...d.knobs.map((k) => `${k.id}${k.next ? '>' : ''}${k.gain ? '+' : ''}`), '|', ...d.ports.map((p) => p.id), '|', ...d.outs.map((o) => o.handle)].join(',');

  // The picture modules' thumbnails: one drawn each live frame, in turn.
  const thumbs = new Thumbs();
  $effect(() => {
    if (live?.frame) untrack(() => thumbs.next(thumbnail));
  });


  // ELK in a web worker (elk-layout.ts), made when first wanted; its last layout by the graph's shape. A change to the
  // shape is laid from the last layout (elk-layout's `before`), so the graph takes it in without being made again (the
  // owner, 71.1: "an optimal layout that accounts for changes while not radically changing itself"); laid afresh when
  // most of what is shown is new (another level, another patch), the other way round, or by Arrange.
  let elk: InstanceType<typeof ElkApi> | null = null;
  let elkDone = $state.raw<{ key: string; laid: ElkNode; portrait: boolean } | null>(null);
  let elkAsked: string | null = null;
  let afresh = $state(false);
  /** ELK failed: the graph's own layout shown rather than nothing. */
  let elkFailed = $state(false);
  let carried = $state(false);
  function lastPlaces(laid: ElkNode): Map<string, XY> {
    const at = new Map<string, XY>();
    const walk = (n: ElkNode) => {
      for (const c of n.children ?? []) {
        if (c.x !== undefined && c.y !== undefined) at.set(c.id, { x: c.x, y: c.y });
        walk(c);
      }
    };
    walk(laid);
    return at;
  }
  function askElk(key: string, graph: ElkNode, before?: (id: string) => XY | undefined) {
    if (elkAsked === key) return;
    elkAsked = key;
    elk ??= new ElkApi({ workerUrl: elkWorker });
    elk
      .layout(graph)
      .then((laid) => {
        if (elkAsked !== key) return;
        elkDone = { key, laid: before ? anchored(laid as ElkNode, before) : (laid as ElkNode), portrait };
        keepLaid(key, elkDone);
      })
      .catch((e) => {
        console.warn('Visuals: the graph could not be laid out', e);
        elkFailed = true;
      });
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
      // Laid before (the rack opened again): at once.
      const kept = elkDone?.key === key ? elkDone : laidBefore.get(key);
      if (kept && elkDone?.key !== key) untrack(() => (elkDone = { key, ...kept }));
      if (kept) Object.assign(g, fromElk(kept.laid, g.nodes, g.edges, (id) => !!placed[id], sz));
      else {
        const last = elkDone && elkDone.portrait === portrait && !afresh ? lastPlaces(elkDone.laid) : null;
        const shown = g.nodes.filter((n) => n.data.kind !== 'lane');
        const carry = !!last && shown.filter((n) => last.has(n.id)).length * 2 > shown.length;
        // Until ELK is back: the last layout's places for what it had (its routes drawn again from them).
        if (carry) Object.assign(g, fromElk(elkDone!.laid, g.nodes, g.edges, (id) => !!placed[id], sz, false));
        untrack(() => {
          const before = carry ? (id: string) => last!.get(id) : undefined;
          askElk(key, toElk(g.nodes, g.edges, sz, { portrait, before }), before);
          afresh = false;
          carried = carry;
        });
      }
    }
    // Every cable over the nodes (EDGE_Z), on its own ports' jacks at every zoom (the owner, 71.1: VCV's way).
    g.edges = g.edges.map((e) => ({ ...e, zIndex: EDGE_Z }));
    untrack(() => {
      // What Svelte Flow knows of each node (its size, selection, a drag under way) carries over; its size not where its
      // sockets changed (another patch's knobs shown), so it is measured again before a cable looks for a new socket.
      const before = new Map(nodes.map((n) => [n.id, n]));
      nodes = g.nodes.map((n) => {
        const b = before.get(n.id);
        const same = b && socketsOf(b.data as NodeData) === socketsOf(n.data);
        return { ...n, measured: same ? b.measured : undefined, selected: b?.selected, ...(b?.dragging ? { position: b.position, dragging: true } : {}) } as Node;
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

  function openNode(id: string) {
    // In select mode a tap anywhere on a node (its name, far out its whole one line) toggles it: Svelte Flow's node
    // click does, once.
    if (selecting) return;
    const d = dataOf(id);
    // A device's title (or its frame): into it; a module's, its page.
    if (d?.kind === 'box' || (d?.kind === 'module' && d.device)) enter(id);
    else if (d?.kind === 'module') onopen?.(path(id));
  }

  setContext<GraphActions>(GRAPH, {
    knob: (module, knob, value) => edit([{ kind: 'knob', module: path(module), knob, value }]),
    knobs: (module, poly, values) =>
      edit(
        values.map((v) => ({ kind: 'knob', module: path(module), knob: v.id, value: v.value })),
        `knob:${path(module).join('/')}.${poly}`,
      ),
    open: openNode,
    enter,
    fine(f) {
      // Let go: the lens stays, settled, with the knob's pin, until a tap elsewhere.
      if (!f && fine) {
        fineEnded = performance.now();
        settled = fine;
      }
      if (f) settled = null;
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
      if (fine || performance.now() - fineEnded < 600) return;
      const d = dataOf(id);
      if (d?.kind !== 'module' || d.type === 'screen') return;
      // A long press: select mode, with it picked (the owner: long press is multi-select).
      holdStill();
      if (!selecting) {
        selecting = true;
        sel = [id];
        selCables = [];
      } else if (!sel.includes(id)) sel = [...sel, id];
    },
    adding: () => (menu = { kind: 'browser' }),
    pinCategory: (scope, category, on) => edit([{ kind: 'pin', scope: [...at, ...scope], category, on }]),
    openCable: openIt,
    cableMenu(id, where) {
      holdStill();
      reroute = null;
      pressedAt = { ...pressedAt, [id]: where };
      if (!selecting) {
        selecting = true;
        sel = [];
        selCables = [id];
      } else if (!selCables.includes(id)) selCables = [...selCables, id];
    },
    rerouting: () => reroute,
    live: () => live?.frame ?? null,
    livePath: (id) => path(id).join('/'),
    thumbs: () => (thumbnail ? thumbs : null),
    portrait: () => portrait,
    routeCables: () => routeCables,
  });

  const ends = () => nodes as unknown as { id: string; data: NodeData }[];
  // A connection where it may go (a cable being rerouted left out of the checks).
  const valid = (c: Connection | Edge) => !!top && connectionV2(c, ends(), top, at, types, reroute ? cableAt(reroute.id).cable : undefined) !== null;
  // A cable dragged from a knob and let go on nothing: a source for the knob to be picked.
  function connectEnd(_: MouseEvent | TouchEvent, state: { fromHandle?: { id?: string | null; nodeId: string; type: string } | null; toHandle?: unknown; to?: XY | null }) {
    const h = state.fromHandle;
    if (!h || state.toHandle || h.type !== 'target' || !h.id?.startsWith('k:') || !state.to) return;
    const d = dataOf(h.nodeId);
    if (d?.kind !== 'module' || !d.knobs.some((k) => knobHandle(k.id) === h.id)) return;
    // The music signals first (a module of one, cabled in, in one change), then the parent's other sources.
    menu = { kind: 'pick', node: h.nodeId, port: h.id.slice(2), at: { x: Math.round(state.to.x), y: Math.round(state.to.y) } };
  }
  /** The music signals that fit a node's input. */
  function signalsInto(node: string, port: string) {
    const d = dataOf(node);
    const into = d?.kind === 'module' ? d.v2.ports.in.find((x) => x.id === port) : undefined;
    return MUSIC_SIGNALS.filter((sg) => fits(sg.type, into?.type ?? 'value', into?.polymorphic));
  }
  /** A music module of `signal` cabled into a node's input, just left of the node, in one change. */
  function musicInto(node: string, port: string, signal: string) {
    const parts = node.split('/');
    const scope = [...at, ...parts.slice(0, -1)];
    const inside = patchAt(patch, scope);
    if (!inside) return;
    const made = musicModuleFor(signal);
    const id = freeModuleId(inside, signal.replace(/[^\w-]/g, '_'));
    edit([
      { kind: 'add', scope, type: made.type, id, knobs: made.knobs },
      { kind: 'connect', scope, from: { module: id, port: SIGNAL_OUT }, to: { module: parts[parts.length - 1], port } },
    ]);
    const n = nodes.find((x) => x.id === node);
    if (n) {
      const key = `${levelKey}|${[...parts.slice(0, -1), id].join('/')}`;
      const where = { x: Math.round(n.position.x - 200), y: Math.round(n.position.y) };
      if (!positions) moved = { ...moved, [key]: where };
      onmove?.(key, where.x, where.y);
    }
  }
  // What is selected: modules and devices, cables (the select mode's, or a desktop's Shift- or Ctrl-clicks); the
  // modules at one level are what "Make device" makes a device of.
  const picked = $derived(nodes.filter((n) => n.selected && (n.data as NodeData).kind === 'module').map((n) => n.id));
  const pickedCables = $derived(edges.filter((e) => e.selected && e.deletable !== false).map((e) => e.id));
  const chosen = $derived.by(() => {
    const scopes = new Set(picked.map((id) => id.split('/').slice(0, -1).join('/')));
    return scopes.size === 1 ? picked : [];
  });
  // The select bar: in select mode, or two or more picked by a desktop's clicks.
  const barShown = $derived(selecting || picked.length + pickedCables.length > 1);
  const pinOf = (id: string, kind: 'module' | 'cable') => {
    const parts = id.split('/');
    return { scope: [...at, ...parts.slice(0, -1)], pin: `${kind}:${parts[parts.length - 1]}` };
  };
  /** Every one picked pinned (or let go), in one change. */
  function pinPicked(on: boolean) {
    edit([...picked.map((id) => pinOf(id, 'module')), ...pickedCables.map((id) => pinOf(id, 'cable'))].map((x) => ({ kind: 'pinOne' as const, ...x, on })));
  }
  const allPinned = $derived(
    picked.length + pickedCables.length > 0 &&
      picked.every((id) => (dataOf(id) as { pinned?: boolean } | undefined)?.pinned) &&
      pickedCables.every((id) => (edges.find((e) => e.id === id)?.data as CableData | undefined)?.pinned),
  );
  // Modules, devices and cables picked, deleted in one change (one undo step); never the Screen.
  const deletable = (id: string) => (dataOf(id) as { type?: string } | undefined)?.type !== 'screen';
  // The devices picked: unmade at once (one change), each as its own menu's Unmake does.
  const pickedDevices = $derived(picked.filter((id) => (dataOf(id) as { device?: unknown } | undefined)?.device));
  const actions = $derived(
    new Set(
      barActions(
        picked.map((id) => {
          const d = dataOf(id) as ModuleData | undefined;
          return { device: !!d?.device, screen: d?.type === 'screen', help: !!(d && helpFor?.(d.type)), scope: id.split('/').slice(0, -1).join('/') };
        }),
        pickedCables.length,
      ),
    ),
  );
  // Where each cable picked by a long press was pressed (its Reroute lifts the end nearer that).
  let pressedAt: Record<string, XY> = {};
  /** The one module or device picked, alone (Open, Help). */
  const onlyModule = $derived(picked.length === 1 && !pickedCables.length ? picked[0] : null);
  const onlyHelp = $derived.by(() => {
    const d = onlyModule ? dataOf(onlyModule) : undefined;
    return d?.kind === 'module' && !d.device && helpFor?.(d.type) ? onlyModule : null;
  });
  function reroutePicked() {
    const id = pickedCables[0];
    const e = edges.find((x) => x.id === id);
    endSelect();
    if (!e) return;
    openCable = null;
    reroute = { id, at: pressedAt[id] ?? { x: 0, y: 0 } };
  }
  function unmakePicked() {
    edit(unmakeEdits(at, pickedDevices));
    endSelect();
  }
  function deletePicked() {
    edit(deleteEdits(at, picked, pickedCables, deletable));
    endSelect();
  }
  // A knob's lens after its fine adjust, settled (its pin): until a tap elsewhere or Escape.
  let settled = $state.raw<Fine | null>(null);
  const settledPin = $derived.by(() => {
    if (!settled) return null;
    const d = dataOf(settled.node);
    if (d?.kind !== 'module' || d.device) return null;
    const parts = settled.node.split('/');
    return { scope: [...at, ...parts.slice(0, -1)], pin: `${parts[parts.length - 1]}.${settled.knob}`, on: !!d.knobs.find((k) => k.id === settled!.knob)?.pinned };
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
    endSelect();
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
  // The cables' opacity (VCV Rack's), this device's: a view option, the Rack's long press.
  const OPACITY_KEY = 'cratarr-graph-cable-opacity';
  let opacity = $state(
    (() => {
      try {
        return cableOpacity(Number(localStorage.getItem(OPACITY_KEY) ?? 1));
      } catch {
        return 1;
      }
    })(),
  );
  function setOpacity(v: number) {
    opacity = cableOpacity(v);
    try {
      localStorage.setItem(OPACITY_KEY, String(opacity));
    } catch {
      /* this visit only */
    }
  }

  // The one preview (the owner, 71.1): of the one module picked, or (nothing picked) the one last tapped.
  let tapped = $state<string | null>(null);
  const preview = $derived.by(() => {
    const id = previewTarget(picked, pickedCables.length, tapped);
    const d = id ? dataOf(id) : undefined;
    const kind = d?.kind === 'module' && !d.device ? previewOf(d.v2.ports) : null;
    return id && d?.kind === 'module' && kind ? { path: path(id).join('/'), label: d.label, kind } : null;
  });

  // The module browser: its search; where a module added goes (the view's centre, or right of the one picked).
  let query = $state('');
  const music = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return MUSIC_SIGNALS.filter((sg) => !q || sg.label.toLowerCase().includes(q) || 'music'.includes(q));
  });
  let vp = { x: 0, y: 0, zoom: 1 };
  function addModule(type: string, knobs: Record<string, number>, base: string) {
    menu = null;
    const inside = top;
    if (!inside) return;
    const id = freeModuleId(inside, base.replace(/[^\w-]/g, '_'));
    edit([{ kind: 'add', scope: [...at], type, id, knobs }]);
    const near = picked.length ? nodes.find((n) => n.id === picked[0] && !n.parentId) : undefined;
    const where = near
      ? { x: Math.round(near.position.x + (near.measured?.width ?? 160) + 60), y: Math.round(near.position.y) }
      : { x: Math.round((width / 2 - vp.x) / vp.zoom - 80), y: Math.round((height / 2 - vp.y) / vp.zoom - 40) };
    const key = `${levelKey}|${id}`;
    if (!positions) moved = { ...moved, [key]: where };
    onmove?.(key, where.x, where.y);
    touched = true;
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape' || menu) return;
    if (reroute) reroute = null;
    else if (settled) settled = null;
    else if (selecting) endSelect();
  }}
  onpointerdown={(e) => {
    if (settled && !(e.target as Element | null)?.closest?.('.lens')) settled = null;
  }}
/>

<div class="patch-graph" class:unlaid={layout === 'elk' && !elkDone && !elkFailed} style:--cable-alpha={drawnOpacity(opacity)} style:--graph-w="{width}px" class:small={zoom < 1} class:far={zoom < FAR} class:narrow={width < 600} style:--z={zoom} bind:clientWidth={width} bind:clientHeight={height}>
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
    {edgeTypes}
    colorMode="dark"
    minZoom={0.05}
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
    multiSelectionKey={['Shift', 'Control', 'Meta']}
    onedgeclick={({ edge, event }) => {
      if (selecting) return toggleCable(edge.id);
      // A Shift- or Ctrl-click only picks it (Svelte Flow's multi-selection); a plain one opens it.
      if (event.shiftKey || event.ctrlKey || event.metaKey) return;
      if (edge.type === 'cable') openIt(edge.id);
    }}
    onnodeclick={({ node, event }) => {
      if (selecting) return togglePick(node.id);
      tapped = node.id;
      // The graph alone: a tap on a module's body opens it too (its knobs, jacks and buttons keep their own taps).
      if (tapOpens && !(event.target as Element | null)?.closest?.('.nodrag, button, input, .svelte-flow__handle')) openNode(node.id);
    }}
    onpaneclick={() => {
      reroute = null;
      tapped = null;
      if (selecting) endSelect();
    }}
  >
    <PatchFit layout={fitKey} active={!touched} {pad} view={{ width, height }} {portrait} onzoom={(z) => (zoom = z)} onview={(v) => (vp = v)} />
    {#if !barShown}
      <Panel position="bottom-left" class="chips-panel">
        <div class="bottom">
          <PatchChips {parts} {padding} onjump={() => (touched = true)} />
          {#if Object.keys(kept).length || carried}<button class="chipbtn" onclick={() => ((afresh = true), onarrange?.())}>Arrange</button>{/if}
        </div>
      </Panel>
    {/if}
    <!-- The way to add a module: always here, whatever the rack holds (the owner, 71.1). -->
    <Panel position="top-left" style={TOP}>
      <div class="topleft">
        <button class="addbtn" aria-label="Add a module" title="Add a module" onclick={() => ((menu = { kind: 'browser' }), (query = ''))}><Icon name="plus" size={22} /></button>
        <!-- The Rack: a tap back to the top; its long press (a right click), the graph's view options. -->
        <nav class="crumbs" aria-label="Devices">
          <button class:here={!at.length} onclick={() => (at = [])} use:longpress={() => (menu = { kind: 'view' })}>Rack</button>
          {#each crumbsOf(patch, at) as name, i (i)}<span>›</span><button class:here={i === at.length - 1} onclick={() => (at = at.slice(0, i + 1))}>{name}</button>{/each}
        </nav>
      </div>
    </Panel>
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
      <!-- Top right, under the legend (clear of the safe area); never with the page's own preview. -->
      {#if preview && !previewShown}
        {#key preview.path}
          <div class="previewbox"><Preview module={preview.path} label={preview.label} kind={preview.kind} live={live?.frame ?? null} thumbs={thumbnail ? thumbs : null} /></div>
        {/key}
      {/if}
    </Panel>
  </SvelteFlow>
  {#if barShown}
    <!-- The app's select bar (lib/SelectBar): how many, and what to do with them at once. -->
    {@const n = picked.length + pickedCables.length}
    <SelectBar label="Selected" count="{n}/{nodes.filter((x) => (x.data as NodeData).kind === 'module').length + edges.filter((e) => e.deletable !== false).length}" onclose={endSelect}>
      <button aria-label={allPinned ? 'Unpin' : 'Pin'} title={allPinned ? 'Unpin' : 'Pin'} disabled={!n} onclick={() => pinPicked(!allPinned)}><Icon name={allPinned ? 'pinOff' : 'pin'} size={24} /></button>
      {#if actions.has('make')}<button aria-label="Make device" title="Make device" onclick={() => (menu = { kind: 'make', ids: chosen })}><Icon name="intoBox" size={24} /></button>{/if}
      {#if actions.has('reroute')}<button aria-label="Reroute" title="Reroute" onclick={reroutePicked}><Icon name="swap" size={24} /></button>{/if}
      {#if actions.has('open') && onlyModule}<button
          aria-label="Open"
          title="Open"
          onclick={() => {
            const id = onlyModule!;
            endSelect();
            onopen?.(path(id));
          }}><Icon name="squareArrowOutUpRight" size={24} /></button
        >{/if}
      {#if actions.has('help') && onlyHelp}<button aria-label="Help" title="Help" onclick={() => (menu = { kind: 'help', id: onlyHelp!, port: '' })}><Icon name="info" size={24} /></button>{/if}
      {#if actions.has('unmake')}<button aria-label="Unmake {pickedDevices.length === 1 ? 'device' : 'devices'}" title="Unmake" onclick={unmakePicked}><Icon name="expand" size={24} /></button>{/if}
      <button aria-label="Delete" title="Delete" disabled={!actions.has('delete')} onclick={deletePicked}><Icon name="trash" size={24} /></button>
    </SelectBar>
  {/if}
</div>

{#if fine}<KnobLens {fine} help={fineHelp} />{:else if settled}<KnobLens
    fine={settled}
    help={null}
    pinned={settledPin?.on}
    onpin={settledPin ? () => edit([{ kind: 'pinOne', scope: settledPin!.scope, pin: settledPin!.pin, on: !settledPin!.on }]) : undefined}
  />{/if}

{#if menu?.kind === 'view'}
  <MenuSheet label="Graph view" title="Graph view" onclose={() => (menu = null)} over>
    <label class="viewopt">
      <span>Cable opacity</span>
      <input type="range" use:panSafe min={CABLE_OPACITY_MIN * 100} max="100" step="5" value={Math.round(opacity * 100)} oninput={(e) => setOpacity(Number((e.currentTarget as HTMLInputElement).value) / 100)} />
      <span class="val">{Math.round(opacity * 100)}%</span>
    </label>
  </MenuSheet>
{:else if menu?.kind === 'help'}
  {@const hid = menu.id}
  {@const port = menu.port}
  {@const d = nodes.find((n) => n.id === hid)?.data as NodeData | undefined}
  {#if d?.kind === 'module'}
    {@const name = port ? (d.knobs.find((k) => k.id === port)?.label ?? d.ports.find((p) => p.id === port)?.label ?? port) : ''}
    {@const title = name ? `${d.label} · ${name}` : d.label}
    <MenuSheet label={title} {title} onclose={() => (menu = null)} over>
      <p class="help">{port ? helpFor?.(d.type, port) : helpFor?.(d.type)}</p>
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
{:else if menu?.kind === 'pick'}
  {@const pick = menu}
  <MenuSheet label="Into {nodeName(pick.node)}" title="Into {nodeName(pick.node)}" onclose={() => (menu = null)} over>
    <div role="menu">
      {#each signalsInto(pick.node, pick.port) as sg (sg.id)}<MenuEntry
          label="Music ▸ {sg.label}"
          onclick={() => {
            menu = null;
            musicInto(pick.node, pick.port, sg.id);
          }}
        />{/each}
      {#if onpick}<MenuEntry
          label="Other sources…"
          onclick={() => {
            menu = null;
            onpick({ from: 'knob', module: path(pick.node), knob: pick.port }, pick.at);
          }}
        />{/if}
    </div>
  </MenuSheet>
{:else if menu?.kind === 'browser'}
  {@const q = query.trim().toLowerCase()}
  <MenuSheet label="Add a module" title="Add a module" onclose={() => (menu = null)} over>
    <div class="browser">
      <!-- svelte-ignore a11y_autofocus -->
      <input class="search" type="search" placeholder="Search" aria-label="Search modules" bind:value={query} autofocus={!coarse} />
      {#each CATEGORIES as cat (cat)}
        {@const list = typesOf(cat, types).filter((t) => !q || labelOf(t).toLowerCase().includes(q))}
        {#if list.length || !q}
          {@const pinned = !!top?.pins.includes(categoryPin(cat))}
          <div class="section">
            <span>{CATEGORY_LABEL[cat]}</span>
            <button class="catpin" class:on={pinned} aria-pressed={pinned} aria-label="Pin all of {CATEGORY_LABEL[cat]}" title="Pin all" onclick={() => edit([{ kind: 'pin', scope: [...at], category: cat, on: !pinned }])}
              ><Icon name="pin" size={16} /></button
            >
          </div>
          <div role="menu">
            {#each list as t (t.type)}<MenuEntry label={labelOf(t)} onclick={() => addModule(t.type, {}, t.type)} />{/each}
          </div>
        {/if}
      {/each}
      {#if music.length}
        <div class="section"><span>Music ▸</span></div>
        <div role="menu">
          {#each music as sg (sg.id)}<MenuEntry label={sg.label} onclick={() => addModule(musicModuleFor(sg.id).type, {}, sg.id)} />{/each}
        </div>
      {/if}
    </div>
  </MenuSheet>
{/if}

<style>
  /* Nothing shown before ELK's first layout of the patch (no makeshift one to jump from). */
  .patch-graph.unlaid :global(.svelte-flow__viewport) {
    visibility: hidden;
  }
  .patch-graph {
    position: relative;
    --mini-h: calc(var(--sheet-h, 0px));
    --patch-mod: #b48cff;
    /* Each wire type's colour (the value's is the modulation purple). */
    --wire-value: var(--patch-mod);
    --wire-trigger: #5fc6ee;
    --wire-audio: #72d39f;
    --wire-light: #f0b04a;
    --wire-points: #e98fc8;
    /* A flow field's: lime, apart from the trigger's cyan and the sound's green. */
    --wire-flow: #c2d94c;
    /* The five categories' colours (rack.ts), on each node's header edge. */
    --cat-sources: #e8a15a;
    --cat-effects: #6fb7ff;
    --cat-mixers: #7ad0a6;
    --cat-modulators: var(--patch-mod);
    --cat-io: color-mix(in srgb, var(--text) 55%, transparent);
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
  /* The modulation cables at the opacity set (the Rack's view options); one in focus at all of it. */
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
  .viewopt {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 12px;
    align-items: center;
    padding: 4px 20px 20px;
    color: var(--text);
    font-size: 15px;
  }
  .viewopt input {
    width: 100%;
    accent-color: var(--accent);
  }
  .viewopt .val {
    min-width: 3.5em;
    text-align: right;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .previewbox {
    display: flex;
    justify-content: flex-end;
    margin-top: 10px;
  }
  .topleft {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  /* The one "+": always over the canvas, top left (clear of the notch). */
  .addbtn {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 1px solid var(--border);
    border-radius: 20px;
    background: var(--float-bg, var(--surface));
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    color: var(--text);
    cursor: pointer;
  }
  .addbtn:hover {
    border-color: color-mix(in srgb, var(--text) 35%, var(--border));
  }
  .browser {
    display: grid;
    padding: 0 0 12px;
  }
  .search {
    margin: 0 20px 8px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    font: inherit;
  }
  .section {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px 4px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .catpin {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 16px;
    background: none;
    color: var(--muted);
    opacity: 0.7;
    cursor: pointer;
  }
  .catpin.on {
    color: var(--accent-ink);
    opacity: 1;
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
  /* Over the bottom: the chips (and Arrange), above an open sheet and the safe area. */
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
  .chipbtn {
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
  .chipbtn:hover {
    color: var(--text);
  }
</style>
