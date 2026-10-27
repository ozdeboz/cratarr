<script lang="ts">
  // Playlists as points, drawn close when they are alike (Insights → Related). What is measured and which links are
  // drawn is in proximity.ts; d3-force only computes positions and Svelte draws the SVG. The points never move (the
  // owner, 2026-10-26: "just no node movement"): the layout is worked out to rest at once, and again when a signal
  // changes, then fitted to the box; panning and pinch-zoom stay. `territories` (lib/territories): the genres drawn as
  // areas under them, each in its colour (the Map's Genres sheet), the points then neutral; `solo` shows one; a tap
  // inside one highlights its playlists.
  import { untrack } from 'svelte';
  import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type Simulation } from 'd3-force';
  import { select } from 'd3-selection';
  import { zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from 'd3-zoom';
  import type { Proximity, Signal } from './api';
  import { longpress } from './longpress';
  import { GROUP_NEIGHBOURS, GROUP_RESOLUTION, backbone, communities, nearest, rankers, spread, weigh } from './proximity';
  import { NAME_SIZE, sharesOf, territories, type Territory } from './territories';

  let {
    data,
    signals,
    selected = $bindable(null),
    groups = $bindable([]),
    onmenu,
    territories: drawnGenres = [],
    solo = null,

  }: {
    data: Proximity;
    signals: Record<Signal, boolean>;
    selected?: number | null;
    /** The colour groups as drawn, for a legend: each colour and its playlists (indexes), largest first. */
    groups?: { colour: string; members: number[] }[];
    /** Long-press or right-click on a point: open that playlist's menu. */
    onmenu?: (i: number, anchor: DOMRect) => void;
    /** Genre territories drawn (the Map's switch). */
    /** The genres drawn as territories, and their colours. */
    territories?: { genre: string; colour: string }[];
    /** The one territory shown (the legend's tap); null: all. */
    solo?: string | null;
    /** The territories' genres and colours as drawn, for the legend. */

  } = $props();

  type Node = { i: number; r: number; x?: number; y?: number; vx?: number; vy?: number };
  /** `r`: the kin link's rank among the kin links (0..1); `bridge`: a tree link that is not kin. */
  type Edge = { key: string; source: Node | number; target: Node | number; r: number; bridge: boolean };

  /** Names shown before zooming in: the best connected playlists. */
  const LABELS = 8;
  /** Group colours: nine of Tableau 10 (made to tell categories apart; its grey left out), one per group. Groups past
   * the palette and lone points stay neutral. */
  const PALETTE = ['#4e79a7', '#f28e2b', '#e15759', '#76b7b2', '#59a14f', '#edc948', '#b07aa1', '#ff9da7', '#9c755f'];

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

  $effect(() => {
    const by = new Map<number, number[]>();
    group.forEach((g, i) => by.set(g, [...(by.get(g) ?? []), i]));
    groups = [...by.entries()].sort((a, b) => a[0] - b[0]).map(([, members]) => ({ colour: colour(members[0]), members }));
  });

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
  /** Bumped when a layout is done: what the drawing below depends on. */
  let frame = $state(0);
  let zoomer: ZoomBehavior<SVGSVGElement, unknown> | undefined;

  let sim: Simulation<Node, Edge> | undefined;

  /** A gentle pull of each point toward the centre of its colour group (lone points are left alone). Measured on the
   * reference library with 9 groups (6 seeds): 0.1 left groups loose on screen (silhouette 0.25, kin among a point's 3
   * nearest 0.41), 0.15 holds them (0.37 / 0.35), 0.2 and more stretch the lines between kin (0.38 / 0.30). */
  const GROUP_PULL = 0.15;
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
      .stop();
    return () => sim?.stop();
  });

  // New signals: the drawn links change on the same running simulation, which is warmed up gently.
  // Kin pulls with its rank (not shared out over a busy point's links, as d3 does by default: measured, that weakened
  // exactly the links inside groups). Bridges are long and slack.
  $effect(() => {
    const edges: Edge[] = [
      ...[...ranked].map(([key, r]) => ({ key, r, bridge: false })),
      ...[...bb.bridges.keys()].map((key) => ({ key, r: 0, bridge: true })),
    ].map(({ key, r, bridge }) => {
      const [a, b] = key.split('-').map(Number);
      return { key, source: a, target: b, r, bridge };
    });
    if (!sim) return;
    sim.force(
      'link',
      forceLink<Node, Edge>(edges)
        .id((p) => p.i)
        .distance((e) => (e.bridge ? 130 : 28 + 60 * (1 - e.r)))
        .strength((e) => {
          if (e.bridge) return 0.03;
          return 0.3 + 0.7 * e.r;
        }),
    );
    // To rest at once, unseen (no gliding), then drawn and fitted.
    sim.alpha(1);
    for (let t = 0; t < 400 && sim.alpha() > sim.alphaMin(); t++) sim.tick();
    // Not read here: `frame++` reads it too, and this effect ran again on its own bump, without end (0.9.8's Map hung
    // the page, the owner 2026-10-27).
    untrack(() => frame++);
    queueMicrotask(fit);
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

  /** The box's size the view was last fitted to. */
  let fitted = { w: 0, h: 0, lands: false };
  /** Each layout, once worked out, and the box resized (full screen, a turned tablet, the page's height measured):
   *  zoomed so every point is in view (the owner, 2026-10-27: it stayed fitted to the size it first had). */
  function fit() {
    if (!svg || !zoomer) return;
    fitted = { w: width, h: height, lands: showTerritories };
    const xs = nodes.map((p) => p.x ?? 0);
    const ys = nodes.map((p) => p.y ?? 0);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    // Room around the points; with the territories, for their areas too (a single point's reaches about 45 out).
    const room = showTerritories ? 150 : 60;
    const k = Math.min(width / (x1 - x0 + room), height / (y1 - y0 + room), 2.5);
    const t = zoomIdentity.scale(k).translate(-(x0 + x1) / 2, -(y0 + y1) / 2);
    select(svg).call(zoomer.transform, t);
  }

  $effect(() => {
    const [w, h, lands] = [width, height, showTerritories];
    if (!frame || (Math.abs(w - fitted.w) < 2 && Math.abs(h - fitted.h) < 2 && lands === fitted.lands)) return;
    untrack(fit);
  });

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

  // The territories (lib/territories): the genres given, on the layout as drawn.
  const shares = $derived(data.playlists.map((p) => sharesOf(p)));
  const showTerritories = $derived(drawnGenres.length > 0);
  const top = $derived(drawnGenres.map((g) => g.genre));
  // The names along the borders keep clear of the points and of the playlists' names shown at rest (the owner,
  // 2026-10-27), as drawn below: a name centred over its point, 11 high.
  const nameBoxes = $derived([
    ...nodes.map((p, i) => [pos[i][0] - p.r, pos[i][1] - p.r, pos[i][0] + p.r, pos[i][1] + p.r] as const),
    ...[...hubs].map((i) => {
      const [x, y] = pos[i];
      const w = data.playlists[i].name.length * 11 * 0.6;
      const base = y - nodes[i].r - 4;
      return [x - w / 2, base - 11, x + w / 2, base + 2] as const;
    }),
  ]);
  const lands = $derived<Territory[]>(showTerritories ? territories(pos, shares, top, nameBoxes) : []);
  const tint = (k: number) => drawnGenres[k]?.colour ?? 'var(--muted)';
  /** A territory tapped: its playlists highlighted (the rest dim); again, or the background, none. */
  let land = $state<string | null>(null);
  // A territory tapped, else the genre shown alone (the legend's tap; the owner, 2026-10-27: the others dimmed).
  const landMembers = $derived(lands.find((l) => l.genre === (land ?? solo))?.members ?? null);

  const dim = (i: number) => (landMembers ? !landMembers.has(i) : selected !== null && i !== selected && !neighbours.has(i));
  const labelled = (i: number) =>
    landMembers ? landMembers.has(i) : selected === null ? hubs.has(i) || view.k > 1.8 : i === selected || neighbours.has(i);
</script>

<div class="wrap" bind:clientWidth={width} bind:clientHeight={height}>
  <!-- A tap on the empty background clears the highlight; Esc on a point does the same from the keyboard. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <svg
    bind:this={svg}
    viewBox={`${-width / 2} ${-height / 2} ${width} ${height}`}
    role="img"
    aria-label="Playlists drawn close together when they are alike"
    onclick={(e) => e.target === svg && ((selected = null), (land = null))}
  >
    <g transform={view.toString()}>
      {#each lands as l, k (l.genre)}
        {#if !solo || solo === l.genre}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <path
            class="land"
            class:on={land === l.genre}
            d={l.path}
            style:--tint={tint(k)}
            stroke-width={1.2 / view.k}
            onclick={() => ((land = land === l.genre ? null : l.genre), (selected = null))}
          />
        {/if}
      {/each}
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
          <circle r={p.r} style:fill={showTerritories ? 'var(--text)' : colour(p.i)} />
          {#if labelled(p.i)}
            <text y={-p.r - 4} style:font-size={`${11 / Math.sqrt(view.k)}px`}>{data.playlists[p.i].name}</text>
          {/if}
        </g>
      {/each}
      <!-- Each territory's name along the inside of its outer borders, at half a playlist's name (the owner,
           2026-10-27: tiny, alongside the border, not across). -->
      {#each lands as l, k (l.genre)}
        {#if !solo || solo === l.genre}
          {#each l.edges as edge, e (e)}
            {@const id = `land-edge-${k}-${e}`}
            <path {id} d={edge.d} fill="none" stroke="none" />
            <path id={`${id}-back`} d={edge.back} fill="none" stroke="none" />
            {#each edge.names as name, m (m)}
              <!-- Clockwise the inside is under the line; the other way round, above it. -->
              <text
                class="land-name"
                class:flip={name.flip}
                style:--tint={tint(k)}
                style:font-size={`${NAME_SIZE / Math.sqrt(view.k)}px`}
                dy={`${(name.flip ? -1.5 : 1.5) / Math.sqrt(view.k)}px`}
                ><textPath href={name.flip ? `#${id}-back` : `#${id}`} startOffset={`${(name.flip ? 1 - name.at : name.at) * 100}%`}>{l.genre}</textPath></text
              >
            {/each}
          {/each}
        {/if}
      {/each}
    </g>
  </svg>
</div>

<style>
  .wrap {
    width: 100%;
    /* As tall as the Map's page leaves it (lib/InsightsMap measures it: nothing to scroll). */
    height: var(--map-h, min(70vh, 620px));
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

  /* A genre's territory: a soft fill in its colour with a thin outline, under the links and points. */
  .land {
    fill: var(--tint);
    fill-opacity: 0.16;
    stroke: var(--tint);
    stroke-opacity: 0.55;
    cursor: pointer;
    transition: fill-opacity 0.2s;
  }

  .land.on {
    fill-opacity: 0.3;
  }

  /* Along the border, on its inside: the border goes clockwise, so below the line is inside. */
  .land-name {
    fill: var(--tint);
    font-weight: 600;
    letter-spacing: 0.04em;
    dominant-baseline: hanging;
    pointer-events: none;
  }

  /* Along the border the other way round: ending where the clockwise one would start, above the line. */
  .land-name.flip {
    dominant-baseline: auto;
    text-anchor: end;
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
