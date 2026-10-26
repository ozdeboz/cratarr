<script lang="ts">
  // Insights' Map (decision 0023; in the panel since 0.8, lib/InsightsPanel): the playlists as a graph, the closest
  // joined. A point selected shows its closest; View opens it over the map (back returns here); a long press its menu.
  // As tall as its page leaves it, nothing below it and nothing to scroll (the owner, 2026-10-26). Genres (a switch in
  // its corner, off until turned on) draws the genre territories (lib/territories), their legend beside it: a tap on a
  // genre shows only its territory, again all.
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

  // The territories' legend, as the graph drew them; the one shown alone.
  let territoryGenres = $state<{ genre: string; colour: string }[]>([]);
  let solo = $state<string | null>(null);
  $effect(() => {
    if (!prefs.mapTerritories) solo = null;
  });

  // The page's height left for the map: from its top to its scroller's foot, then less by whatever would still scroll.
  let mapH = $state<number | null>(null);
  function fill(node: HTMLElement) {
    const scroller = () => {
      for (let e = node.parentElement; e; e = e.parentElement) {
        const o = getComputedStyle(e).overflowY;
        if ((o === 'auto' || o === 'scroll') && e.clientHeight < e.scrollHeight + 1) return e;
      }
      return null;
    };
    let frame = 0;
    const measure = () => {
      const s = scroller();
      const top = node.getBoundingClientRect().top;
      const foot = s ? s.getBoundingClientRect().bottom - parseFloat(getComputedStyle(s).paddingBottom) : window.innerHeight;
      const h = Math.max(240, Math.floor(foot - top - 8));
      mapH = h;
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const box = s ?? document.documentElement;
        const over = box.scrollHeight - box.clientHeight;
        if (over > 0) mapH = Math.max(240, h - over);
      });
    };
    measure();
    const later = setTimeout(measure, 300);
    addEventListener('resize', measure);
    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(later);
      removeEventListener('resize', measure);
    };
  }


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
    <div class="stage" {@attach fill} style:--map-h={mapH ? `${mapH}px` : null}>
      <div class="box">
        {#key insights.removed.join('/')}
          <ProximityGraph
            data={shown}
            signals={prefs.signals}
            bind:selected
            bind:groups
            bind:genres={territoryGenres}
            showTerritories={prefs.mapTerritories}
            {solo}
            onmenu={(i, anchor) => (menu = { name: shown.playlists[i].name, anchor })}
          />
        {/key}
        {#if me}
          <button class="corner" onclick={() => viewOver(me.name)}><Icon name="squareArrowOutUpRight" size={18} /><span>View {me.name}</span></button>
        {/if}
        <!-- The legend explains the graph: always at hand, in the graph's corner. -->
        <button class="corner legend-btn" onclick={() => (legendOpen = true)}><Icon name="info" size={18} /><span>Legend</span></button>
        <!-- Genres: the territories on or off; on, their genres, a tap showing one alone. -->
        <div class="lands">
          <button class="corner-left" class:on={prefs.mapTerritories} aria-pressed={prefs.mapTerritories} onclick={() => (prefs.mapTerritories = !prefs.mapTerritories)}
            ><Icon name="globe" size={18} /><span>Genres</span></button
          >
          {#if prefs.mapTerritories}
            {#each territoryGenres as g (g.genre)}
              <button class="chip" class:off={solo !== null && solo !== g.genre} aria-pressed={solo === g.genre} onclick={() => (solo = solo === g.genre ? null : g.genre)}
                ><i style:background={g.colour}></i>{g.genre}</button
              >
            {/each}
          {/if}
        </div>
      </div>
    </div>
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

  /* Genres, top left, and on, its genres under it. */
  .lands {
    position: absolute;
    top: 10px;
    left: 10px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    max-width: 45%;
  }

  .corner-left {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 40px;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font-weight: 600;
  }

  .corner-left.on {
    border-color: var(--accent);
  }

  .chip {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 30px;
    max-width: 100%;
    padding: 0 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 0.85rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip i {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .chip.off {
    opacity: 0.45;
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
  .empty {
    color: var(--muted);
  }

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
