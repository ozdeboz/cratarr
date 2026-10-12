<script lang="ts">
  // Playlists as points, drawn close when they are alike (Insights → Related). What is measured and which links are
  // drawn is in proximity.ts; d3-force only computes positions and Svelte draws the SVG. Switching a signal swaps the
  // links of the running simulation and warms it up, so the points glide to their new places.
  import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type Simulation } from 'd3-force';
  import { select } from 'd3-selection';
  import { zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from 'd3-zoom';
  import type { Proximity, Signal } from './api';
  import { backbone, groups, rankers, weigh } from './proximity';

  let {
    data,
    signals,
    selected = $bindable(null),
  }: { data: Proximity; signals: Record<Signal, boolean>; selected?: number | null } = $props();

  type Node = { i: number; r: number; x?: number; y?: number; vx?: number; vy?: number };
  type Edge = { key: string; source: Node | number; target: Node | number; w: number };

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

  const drawn = $derived(backbone(n, weigh(data, rank, signals)));
  const group = $derived(groups(n, drawn));
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

  $effect(() => {
    sim = forceSimulation<Node, Edge>(nodes)
      .force('charge', forceManyBody<Node>().strength(-80).distanceMax(320))
      .force('collide', forceCollide<Node>((p) => p.r + 2))
      .force('x', forceX<Node>(0).strength(0.05))
      .force('y', forceY<Node>(0).strength(0.05))
      .on('tick', () => frame++)
      .on('end', fit);
    return () => sim?.stop();
  });

  // New signals: the drawn links change on the same running simulation, which is warmed up gently.
  $effect(() => {
    const edges: Edge[] = [...drawn].map(([key, w]) => {
      const [a, b] = key.split('-').map(Number);
      return { key, source: a, target: b, w };
    });
    if (!sim) return;
    sim.force(
      'link',
      forceLink<Node, Edge>(edges)
        .id((p) => p.i)
        .distance((e) => 26 + 70 * (1 - e.w))
        .strength(0.7),
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
        {@const w = drawn.get(`${l.a}-${l.b}`)}
        {#if w !== undefined}
          <line
            class="link"
            class:hot={selected !== null && (l.a === selected || l.b === selected)}
            x1={pos[l.a][0]}
            y1={pos[l.a][1]}
            x2={pos[l.b][0]}
            y2={pos[l.b][1]}
            stroke-width={(0.5 + 2 * w) / Math.sqrt(view.k)}
            style:opacity={dim(l.a) || dim(l.b) ? 0.06 : 0.2 + 0.35 * w}
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
