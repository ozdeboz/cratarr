<script lang="ts">
  // Insights' Map (decision 0023; in the panel since 0.8, lib/InsightsPanel): the playlists as a graph, the closest
  // joined. A point selected shows its closest; View opens it over the map (back returns here); a long press its menu.
  import { type Signal } from './api';
  import { insights, plural } from './insights.svelte';
  import { live } from './live.svelte';
  import Icon from './Icon.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { viewOver } from './panel.svelte';
  import PlaylistMenu from './PlaylistMenu.svelte';
  import { prefs } from './prefs.svelte';
  import PageLoading from './PageLoading.svelte';
  import ProximityGraph from './ProximityGraph.svelte';

  const data = $derived(insights.data);
  const error = $derived(insights.error);
  const shown = $derived(insights.shown);

  let selected = $state<number | null>(null);
  // The playlist menu, opened by a long-press on a point; `name` is the playlist's id.
  let menu = $state<{ name: string; anchor: DOMRect } | null>(null);

  /** What the menu shows for a playlist: the live counts when the shell has them. */
  const menuPlaylist = (name: string) => {
    const known = live.view?.playlists.find((x) => x.id === name);
    const point = data?.proximity.playlists.find((x) => x.name === name);
    return known ?? { id: name, name, count: point?.tracks ?? 0, dead: 0 };
  };

  const me = $derived(shown && selected !== null ? (shown.playlists[selected] ?? null) : null);

  // The legend: a sheet from the graph's button (it explains the graph; it is not about a playlist).
  let legendOpen = $state(false);
  let groups = $state<{ colour: string; members: number[] }[]>([]);
  /** A group's name: the genres most common among its playlists' top genres. */
  function groupName(members: number[]) {
    if (!shown) return '';
    const count = new Map<string, number>();
    for (const i of members) for (const g of shown.playlists[i]?.genres ?? []) count.set(g, (count.get(g) ?? 0) + 1);
    const top = [...count].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, 5).map(([g]) => g);
    return top.length ? top.join(', ') : 'No genre tags';
  }

  const SIGNALS: { id: Signal; label: string }[] = [
    { id: 'tracks', label: 'Shared tracks' },
    { id: 'genre', label: 'Genre' },
    { id: 'artists', label: 'Artists' },
  ];
  const anyOn = $derived(SIGNALS.some((s) => prefs.signals[s.id]));


</script>

<div class="map">
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !data}
    <PageLoading />
  {:else if !shown || shown.playlists.length < 2}
    <p class="empty">Related playlists need at least two playlists.</p>
  {:else if !anyOn}
    <p class="empty">Turn on at least one of shared tracks, genre or artists.</p>
  {:else}
    <div class="stage">
      <div class="box">
        {#key insights.removed.join('/')}
          <ProximityGraph
            data={shown}
            signals={prefs.signals}
            bind:selected
            bind:groups
            onmenu={(i, anchor) => (menu = { name: shown.playlists[i].name, anchor })}
          />
        {/key}
        {#if me}
          <button class="corner" onclick={() => viewOver(me.name)}><Icon name="squareArrowOutUpRight" size={18} /><span>View {me.name}</span></button>
        {/if}
        <!-- The legend explains the graph: always at hand, in the graph's corner. -->
        <button class="corner legend-btn" onclick={() => (legendOpen = true)}><Icon name="info" size={18} /><span>Legend</span></button>
      </div>
    </div>
    {#if !me}
      <p class="hint">Tap a playlist to highlight its closest playlists. Pinch or scroll to zoom.</p>
    {/if}
  {/if}
</div>

{#if menu}
  <PlaylistMenu
    playlists={[menuPlaylist(menu.name)]}
    onclose={() => (menu = null)}
    ondeleted={() => (selected = null)}
  />
{/if}

{#if legendOpen}
  <MenuSheet label="Legend" title="Legend" onclose={() => (legendOpen = false)}>
    <div class="legend">
      <h3>Lines</h3>
      <ul class="key">
        <li>
          <svg width="44" height="14" aria-hidden="true"><line x1="2" y1="7" x2="42" y2="7" class="solid thick" /></svg>
          <span>Close: each other's closest playlists, or one's own closest. Thicker and shorter is closer.</span>
        </li>
        <li>
          <svg width="44" height="14" aria-hidden="true"><line x1="2" y1="7" x2="42" y2="7" class="solid" /></svg>
          <span>Less close, but still among the closest.</span>
        </li>
        <li>
          <svg width="44" height="14" aria-hidden="true"><line x1="2" y1="7" x2="42" y2="7" class="dashed" /></svg>
          <span>Only there to connect everything: barely alike.</span>
        </li>
      </ul>
      <h3>Points</h3>
      <ul class="key">
        <li>
          <svg width="44" height="24" aria-hidden="true"><circle cx="10" cy="12" r="4" class="dot" /><circle cx="30" cy="12" r="10" class="dot" /></svg>
          <span>A playlist; bigger has more tracks.</span>
        </li>
      </ul>
      <h3>Colours</h3>
      <p class="muted">Groups of playlists that are alike, named by their most common genres.</p>
      <ul class="key">
        {#each groups as g, gi (gi)}
          <li>
            <span class="swatch" style:background={g.colour}></span>
            <span>{groupName(g.members)} <span class="muted">· {plural(g.members.length, 'playlist')}</span></span>
          </li>
        {/each}
      </ul>
    </div>
  </MenuSheet>
{/if}

<style>
  /* The graph keeps its width (at most 900 px). */
  .stage {
    container-type: inline-size;
  }

  .box {
    position: relative;
    width: min(100%, 900px);
  }

  /* A button in the graph's corner: View (top), Legend (bottom). */
  .corner {
    position: absolute;
    top: 10px;
    right: 10px;
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: calc(100% - 20px);
    min-height: 40px;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font-weight: 600;
  }
  .corner.legend-btn {
    top: auto;
    bottom: 10px;
  }


  .corner span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .legend {
    padding: 0 16px 12px;
  }

  .key {
    margin: 0 0 6px;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 8px;
    font-size: 0.9rem;
  }

  .key li {
    display: grid;
    grid-template-columns: 44px 1fr;
    gap: 10px;
    align-items: center;
  }

  .key line {
    stroke: var(--muted);
    stroke-linecap: round;
    stroke-width: 1.2;
  }

  .key line.thick {
    stroke-width: 3;
  }

  .key line.dashed {
    stroke-width: 0.9;
    stroke-dasharray: 4 4;
  }

  .key .dot {
    fill: var(--muted);
  }

  .swatch {
    justify-self: center;
    width: 14px;
    height: 14px;
    border-radius: 50%;
  }

  button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--surface);
    border: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .muted,
  .hint,
  .empty {
    color: var(--muted);
  }

  .hint,
  .empty {
    font-size: 0.9rem;
  }

  .empty {
    margin-top: 32px;
  }

  .error {
    color: var(--danger-ink);
  }

</style>
