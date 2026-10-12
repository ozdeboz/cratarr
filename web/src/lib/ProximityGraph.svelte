<script lang="ts">
  // Playlists as points, drawn close when they are alike (Insights → Related). What is measured and which links are
  // drawn is in proximity.ts; d3-force only computes positions and Svelte draws the SVG. Switching a signal swaps the
  // links of the running simulation and warms it up, so the points glide to their new places.
  import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type Simulation } from 'd3-force';
  import { select } from 'd3-selection';
  import { zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from 'd3-zoom';
  import type { Proximity, Signal } from './api';
  import { longpress } from './longpress';
  import { GROUP_NEIGHBOURS, GROUP_RESOLUTION, backbone, communities, nearest, rankers, spread, weigh } from './proximity';

  let {
    data,
    signals,
    selected = $bindable(null),
    onmenu,
  }: {
    data: Proximity;
    signals: Record<Signal, boolean>;
    selected?: number | null;
    /** Long-press or right-click on a point: open that playlist's menu. */
    onmenu?: (i: number, anchor: DOMRect) => void;
  } = $props();

  type Node = { i: number; r: number; x?: number; y?: number; vx?: number; vy?: number };
  /** `r`: the kin link's rank among the kin links (0..1); `bridge`: a tree link that is not kin; `anchor`: a bridge
   * that is one of its playlists' closest link. */
  type Edge = { key: string; source: Node | number; target: Node | number; r: number; bridge: boolean; anchor: boolean };

  /** Names shown before zooming in: the best connected playlists. */
  const LABELS = 8;
  /** Group colours, readable on both themes; groups past the palette and lone points stay neutral. */
  const PALETTE = ['#4cc38a', '#5b9cf5', '#f2a33a', '#d46bd6', '#e5615b', '#38c3d6', '#b8c94a', '#9a87f5'];

  // The parent remounts this component when the data changes ({#key}), so reading it once is right.
  // svelte-ignore state_referenced_locally
  const n = data.playlists.length;
  // svelte-ignore state_referenced_locally
  const rank = rankers(data);
  // Size grows slowly with the number of tracks, so a big playlist does not cover its neighbours.
  // svelte-ignore state_referenced_locally
  const nodes: Node[] = data.playlists.map((p, i) => ({ i, r: Math.min(11, 3 + 1.4 * Math.log2(1 + p.tracks / 8)) }));

  const weights = $derived(weigh(data, rank, signals));
  const bb = $derived(backbone(n, weights));
  /** Every drawn link: kin by its spread rank, bridges apart. */
  const ranked = $derived(spread(bb.kin));
  const drawn = $derived(new Map([...ranked, ...bb.bridges]));
  // Colour groups come from all closeness (each playlist's nearest few), not only the drawn lines.
  const group = $derived(communities(n, nearest(n, weights, GROUP_NEIGHBOURS), GROUP_RESOLUTION));
  const groupSize = $derived(group.reduce((m, g) => m.set(g, (m.get(g) ?? 0) + 1), new Map<number, number>()));
  const colour = (i: number) => (groupSize.get(group[i])! > 1 && group[i] < PALETTE.length ? PALETTE[group[i]] : 'var(--muted)');

  const degree = $derived.by(() => {
    const d = new Array(n).fill(0);
    for (const k of drawn.keys()) for (const x of k.split('-').map(Number)) d[x]++;
    return d;
  });
  const hubs = $derived(new Set([...Array(n).keys()].sort((a, b) => degree[b] - degree[a] || data.playlists[b].tracks - data.playlists[a].tracks).slice(0, LABELS)));

  let width = $state(600);
  let height = $state(480);
  let svg = $state<SVGSVGElement>();
  let view = $state<ZoomTransform>(zoomIdentity);
  /** Bumped on every simulation tick: what the drawing below depends on. */
  let frame = $state(0);
  let zoomer: ZoomBehavior<SVGSVGElement, unknown> | undefined;
  let fitted = false;

  let sim: Simulation<Node, Edge> | undefined;

  /** A gentle pull of each point toward the centre of its colour group (lone points are left alone). Measured on the
   * reference library, it is what holds groups together most (silhouette -0.07 to 0.09, kin among the 3 nearest
   * points 0.51 to 0.59), at a small cost in how exactly distance follows link strength. */
  const GROUP_PULL = 0.1;
  function pullToGroups(alpha: number) {
    const centre = new Map<number, { x: number; y: number; n: number }>();
    for (const p of nodes) {
      const c = centre.get(group[p.i]) ?? { x: 0, y: 0, n: 0 };
      c.x += p.x ?? 0;
      c.y += p.y ?? 0;
      c.n++;
      centre.set(group[p.i], c);
    }
    for (const p of nodes) {
      const c = centre.get(group[p.i])!;
      if (c.n < 2) continue;
      p.vx = (p.vx ?? 0) + (c.x / c.n - (p.x ?? 0)) * GROUP_PULL * alpha;
      p.vy = (p.vy ?? 0) + (c.y / c.n - (p.y ?? 0)) * GROUP_PULL * alpha;
    }
  }

  $effect(() => {
    sim = forceSimulation<Node, Edge>(nodes)
      .force('charge', forceManyBody<Node>().strength(-80).distanceMax(320))
      .force('collide', forceCollide<Node>((p) => p.r + 2))
      .force('x', forceX<Node>(0).strength(0.05))
      .force('y', forceY<Node>(0).strength(0.05))
      .force('group', pullToGroups)
      .on('tick', () => frame++)
      .on('end', fit);
    return () => sim?.stop();
  });

  // New signals: the drawn links change on the same running simulation, which is warmed up gently.
  // Kin pulls with its rank (not shared out over a busy point's links, as d3 does by default: measured, that weakened
  // exactly the links inside groups). Bridges are long and slack; anchors keep a normal length.
  $effect(() => {
    const edges: Edge[] = [
      ...[...ranked].map(([key, r]) => ({ key, r, bridge: false })),
      ...[...bb.bridges.keys()].map((key) => ({ key, r: 0, bridge: true })),
    ].map(({ key, r, bridge }) => {
      const [a, b] = key.split('-').map(Number);
      return { key, source: a, target: b, r, bridge, anchor: bb.anchors.has(key) };
    });
    if (!sim) return;
    sim.force(
      'link',
      forceLink<Node, Edge>(edges)
        .id((p) => p.i)
        .distance((e) => (e.anchor ? 58 : e.bridge ? 130 : 28 + 60 * (1 - e.r)))
        .strength((e) => {
          if (e.anchor) return 0.3;
          if (e.bridge) return 0.03;
          return 0.3 + 0.7 * e.r;
        }),
    );
    sim.alpha(Math.max(sim.alpha(), 0.6)).restart();
  });

  // Pan and pinch-zoom.
  $effect(() => {
    if (!svg) return;
    zoomer = zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.3, 5])
      .on('zoom', (e) => (view = e.transform));
    select(svg).call(zoomer);
    return () => select(svg!).on('.zoom', null);
  });

  /** Once, when the first layout has settled: zoom so every point is in view. */
  function fit() {
    if (fitted || !svg || !zoomer) return;
    fitted = true;
    const xs = nodes.map((p) => p.x ?? 0);
    const ys = nodes.map((p) => p.y ?? 0);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const k = Math.min(width / (x1 - x0 + 60), height / (y1 - y0 + 60), 2.5);
    const t = zoomIdentity.scale(k).translate(-(x0 + x1) / 2, -(y0 + y1) / 2);
    select(svg).call(zoomer.transform, t);
  }

  const neighbours = $derived.by(() => {
    const s = new Set<number>();
    if (selected === null) return s;
    for (const k of drawn.keys()) {
      const [a, b] = k.split('-').map(Number);
      if (a === selected) s.add(b);
      if (b === selected) s.add(a);
    }
    return s;
  });

  // Positions as the drawing reads them (depends on `frame`).
  const pos = $derived.by(() => {
    void frame;
    return nodes.map((p) => [p.x ?? 0, p.y ?? 0] as const);
  });

  const dim = (i: number) => selected !== null && i !== selected && !neighbours.has(i);
  const labelled = (i: number) => (selected === null ? hubs.has(i) || view.k > 1.8 : i === selected || neighbours.has(i));
</script>

<div class="wrap" bind:clientWidth={width} bind:clientHeight={height}>
  <!-- A tap on the empty background clears the highlight; Esc on a point does the same from the keyboard. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <svg
    bind:this={svg}
    viewBox={`${-width / 2} ${-height / 2} ${width} ${height}`}
    role="img"
    aria-label="Playlists drawn close together when they are alike"
    onclick={(e) => e.target === svg && (selected = null)}
  >
    <g transform={view.toString()}>
      {#each data.links as l (`${l.a}-${l.b}`)}
        {@const k = `${l.a}-${l.b}`}
        {@const r = ranked.get(k)}
        {#if r !== undefined || bb.bridges.has(k)}
          <line
            class="link"
            class:bridge={r === undefined}
            class:hot={selected !== null && (l.a === selected || l.b === selected)}
            x1={pos[l.a][0]}
            y1={pos[l.a][1]}
            x2={pos[l.b][0]}
            y2={pos[l.b][1]}
            stroke-width={(r === undefined ? 0.7 : 0.6 + 2.6 * r) / Math.sqrt(view.k)}
            stroke-dasharray={r === undefined ? `${4 / Math.sqrt(view.k)} ${4 / Math.sqrt(view.k)}` : undefined}
            style:opacity={dim(l.a) || dim(l.b) ? 0.06 : r === undefined ? 0.3 : 0.2 + 0.55 * r}
          />
        {/if}
      {/each}
      {#each nodes as p (p.i)}
        <g
          class="point"
          class:dim={dim(p.i)}
          class:sel={p.i === selected}
          transform={`translate(${pos[p.i][0]},${pos[p.i][1]})`}
          role="button"
          tabindex="0"
          aria-label={data.playlists[p.i].name}
          onclick={() => (selected = selected === p.i ? null : p.i)}
          use:longpress={(anchor) => onmenu?.(p.i, anchor)}
          onkeydown={(e) =>
            e.key === 'Escape' ? (selected = null) : (e.key === 'Enter' || e.key === ' ') && (selected = selected === p.i ? null : p.i)}
        >
          <circle r={p.r} style:fill={colour(p.i)} />
          {#if labelled(p.i)}
            <text y={-p.r - 4} style:font-size={`${11 / Math.sqrt(view.k)}px`}>{data.playlists[p.i].name}</text>
          {/if}
        </g>
      {/each}
    </g>
  </svg>
</div>

<style>
  .wrap {
    width: 100%;
    height: min(70vh, 620px);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    overflow: hidden;
  }

  svg {
    display: block;
    width: 100%;
    height: 100%;
    touch-action: none;
    cursor: grab;
  }

  .link {
    stroke: var(--muted);
    stroke-linecap: round;
    transition: opacity 0.3s;
    pointer-events: none;
  }

  .link.hot {
    stroke: var(--text);
  }

  .point {
    cursor: pointer;
    transition: opacity 0.3s;
    outline: none;
  }

  .point circle {
    fill-opacity: 0.9;
    stroke: var(--surface);
    stroke-width: 1.2;
  }

  .point.sel circle,
  .point:focus-visible circle {
    fill-opacity: 1;
    stroke: var(--text);
    stroke-width: 2;
  }

  .point.dim {
    opacity: 0.15;
  }

  .point text {
    fill: var(--text);
    text-anchor: middle;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
    stroke-linejoin: round;
    pointer-events: none;
  }
</style>
