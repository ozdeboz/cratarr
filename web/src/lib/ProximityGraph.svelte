<script lang="ts">
  // Playlists as points, pulled together by how close they are (Insights → Related). d3-force only computes positions;
  // Svelte draws the SVG. Switching a signal changes the link forces of the running simulation and warms it up, so
  // the points glide to their new places instead of the graph being drawn again.
  import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type Simulation } from 'd3-force';
  import { select } from 'd3-selection';
  import { zoom, zoomIdentity, type ZoomTransform } from 'd3-zoom';
  import type { Proximity, Signal } from './api';

  let {
    data,
    signals,
    selected = $bindable(null),
  }: { data: Proximity; signals: Record<Signal, boolean>; selected?: number | null } = $props();

  type Node = { i: number; r: number; x?: number; y?: number; vx?: number; vy?: number };
  type Edge = { key: string; source: Node | number; target: Node | number; w: number };

  /** A link is drawn from this weight; each point keeps at most its `PER_POINT` strongest. */
  const MIN_WEIGHT = 0.08;
  const PER_POINT = 6;
  /** Names are shown for this many of the biggest playlists (and always for the highlighted ones). */
  const LABELS = 12;

  /** The weight of a pair: the mean of the switched-on signals. */
  function weight(l: Proximity['links'][number]) {
    const on = (['tracks', 'genre', 'artists'] as const).filter((s) => signals[s]);
    return on.length ? on.reduce((sum, s) => sum + l[s], 0) / on.length : 0;
  }

  let width = $state(600);
  let height = $state(480);
  let svg = $state<SVGSVGElement>();
  let view = $state<ZoomTransform>(zoomIdentity);
  /** Bumped on every simulation tick: what the drawing below depends on. */
  let frame = $state(0);

  // The parent remounts this component when the data changes ({#key}), so reading it once is right.
  // svelte-ignore state_referenced_locally
  const nodes: Node[] = data.playlists.map((p, i) => ({ i, r: 5 + Math.min(Math.sqrt(p.tracks), 18) }));

  /** The links worth drawing under the current signals, keyed by pair. */
  const shown = $derived.by(() => {
    const weighted = data.links.map((l) => ({ l, w: weight(l) })).filter((x) => x.w >= MIN_WEIGHT);
    weighted.sort((a, b) => b.w - a.w);
    const per = new Map<number, number>();
    const keep = new Map<string, number>();
    for (const { l, w } of weighted) {
      if ((per.get(l.a) ?? 0) >= PER_POINT && (per.get(l.b) ?? 0) >= PER_POINT) continue;
      per.set(l.a, (per.get(l.a) ?? 0) + 1);
      per.set(l.b, (per.get(l.b) ?? 0) + 1);
      keep.set(`${l.a}-${l.b}`, w);
    }
    return keep;
  });

  let sim: Simulation<Node, Edge> | undefined;

  $effect(() => {
    sim = forceSimulation<Node, Edge>(nodes)
      .force('charge', forceManyBody<Node>().strength(-140))
      .force('collide', forceCollide<Node>((n) => n.r + 6))
      .force('x', forceX<Node>(0).strength(0.06))
      .force('y', forceY<Node>(0).strength(0.06))
      .on('tick', () => frame++);
    return () => sim?.stop();
  });

  // New signals: new link forces on the same running simulation, and a gentle reheat.
  $effect(() => {
    const edges: Edge[] = [...shown].map(([key, w]) => {
      const [a, b] = key.split('-').map(Number);
      return { key, source: a, target: b, w };
    });
    if (!sim) return;
    sim.force(
      'link',
      forceLink<Node, Edge>(edges)
        .id((n) => n.i)
        .distance((e) => 50 + 170 * (1 - e.w))
        .strength((e) => 0.15 + 0.85 * e.w),
    );
    sim.alpha(Math.max(sim.alpha(), 0.5)).restart();
  });

  // Pan and pinch-zoom; a tap on the empty background clears the highlight.
  $effect(() => {
    if (!svg) return;
    const z = zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.3, 4])
      .on('zoom', (e) => (view = e.transform));
    select(svg).call(z);
    return () => select(svg!).on('.zoom', null);
  });

  const neighbours = $derived.by(() => {
    const s = new Set<number>();
    if (selected === null) return s;
    for (const key of shown.keys()) {
      const [a, b] = key.split('-').map(Number);
      if (a === selected) s.add(b);
      if (b === selected) s.add(a);
    }
    return s;
  });

  // svelte-ignore state_referenced_locally
  const biggest = new Set(
    [...data.playlists.keys()].sort((a, b) => data.playlists[b].tracks - data.playlists[a].tracks).slice(0, LABELS),
  );

  // Positions as the drawing reads them (depends on `frame`).
  const pos = $derived.by(() => {
    void frame;
    return nodes.map((n) => [n.x ?? 0, n.y ?? 0] as const);
  });

  const dim = (i: number) => selected !== null && i !== selected && !neighbours.has(i);
  const labelled = (i: number) => (selected === null ? biggest.has(i) || view.k > 1.6 : i === selected || neighbours.has(i));
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
        {@const w = shown.get(`${l.a}-${l.b}`)}
        <line
          class="link"
          class:on={w !== undefined}
          class:hot={selected !== null && (l.a === selected || l.b === selected)}
          x1={pos[l.a][0]}
          y1={pos[l.a][1]}
          x2={pos[l.b][0]}
          y2={pos[l.b][1]}
          stroke-width={w === undefined ? 0 : 0.6 + 4 * w}
          style:opacity={w === undefined ? 0 : dim(l.a) || dim(l.b) ? 0.08 : 0.25 + 0.6 * w}
        />
      {/each}
      {#each nodes as n (n.i)}
        <g
          class="point"
          class:dim={dim(n.i)}
          class:sel={n.i === selected}
          transform={`translate(${pos[n.i][0]},${pos[n.i][1]})`}
          role="button"
          tabindex="0"
          aria-label={data.playlists[n.i].name}
          onclick={() => (selected = selected === n.i ? null : n.i)}
          onkeydown={(e) =>
            e.key === 'Escape' ? (selected = null) : (e.key === 'Enter' || e.key === ' ') && (selected = selected === n.i ? null : n.i)}
        >
          <circle r={n.r} />
          {#if labelled(n.i)}
            <text y={-n.r - 5}>{data.playlists[n.i].name}</text>
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
    transition:
      opacity 0.4s,
      stroke-width 0.4s;
    pointer-events: none;
  }

  .link.hot {
    stroke: var(--accent);
  }

  .point {
    cursor: pointer;
    transition: opacity 0.3s;
    outline: none;
  }

  .point circle {
    fill: var(--accent);
    fill-opacity: 0.75;
    stroke: var(--bg);
    stroke-width: 1.5;
  }

  .point.sel circle {
    fill-opacity: 1;
    stroke: var(--text);
    stroke-width: 2.5;
  }

  .point:focus-visible circle {
    stroke: var(--text);
    stroke-width: 2.5;
  }

  .point.dim {
    opacity: 0.2;
  }

  .point text {
    fill: var(--text);
    font-size: 11px;
    text-anchor: middle;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
    stroke-linejoin: round;
    pointer-events: none;
  }
</style>
