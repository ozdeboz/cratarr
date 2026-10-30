<script lang="ts">
  // Insights' Map (decision 0023; in the panel since 0.8, lib/InsightsPanel): the playlists as a graph, the closest
  // joined. A point selected shows its closest; View opens it over the map (back returns here); a long press its menu.
  // As tall as its page leaves it, nothing below it and nothing to scroll (the owner, 2026-10-26). Genres, in its bottom left
  // corner, opens its sheet (the owner, 2026-10-27: not on the map all the time): the territories on or off
  // (lib/territories), which genres they show (ticked, up to MAX_GENRES, each its colour), and one shown alone. Full
  // screen (its panel's bar) lays it over everything; its corner, Esc or the browser's own leaving ends it.
  import { type Signal } from './api';
  import { insights, mapFull, plural } from './insights.svelte';
  import { GENRE_PALETTE, genreRanking, MAX_GENRES, sharesOf, TERRITORIES } from './territories';
  import ChoiceSheet from './settings/ChoiceSheet.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
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

  // The territories: every genre there is one of (most share first), those ticked (at first the most shared six), each
  // in its colour by its place among them; and the one shown alone.
  const ranking = $derived(shown ? genreRanking(shown.playlists.map((p) => sharesOf(p))) : []);
  const ticked = $derived(prefs.mapGenres ?? ranking.slice(0, TERRITORIES).map((g) => g.genre));
  const colourOf = (genre: string) => GENRE_PALETTE[ticked.indexOf(genre) % GENRE_PALETTE.length];
  const drawn = $derived(
    prefs.mapTerritories ? ticked.filter((g) => ranking.some((r) => r.genre === g)).map((genre) => ({ genre, colour: colourOf(genre) })) : [],
  );
  function tick(genre: string, on: boolean) {
    const now = ticked.filter((g) => g !== genre);
    prefs.mapGenres = on ? [...now, genre].slice(0, MAX_GENRES) : now;
    if (!on && solo === genre) solo = null;
  }
  let solo = $state<string | null>(null);
  $effect(() => {
    if (!prefs.mapTerritories) solo = null;
  });
  let genresOpen = $state(false);
  let choosingSolo = $state(false);

  // Full screen: over everything here, and the browser's own where it has one (as the player's, lib/NowPlaying).
  const canFullscreen = typeof document !== 'undefined' && !!document.fullscreenEnabled && typeof document.documentElement.requestFullscreen === 'function';
  $effect(() => {
    if (!mapFull.on) return;
    if (canFullscreen && !document.fullscreenElement) document.documentElement.requestFullscreen({ navigationUI: 'hide' }).catch(() => {});
    let entered = false;
    const changed = () => {
      if (document.fullscreenElement) entered = true;
      else if (entered) mapFull.on = false;
    };
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (mapFull.on = false);
    document.addEventListener('fullscreenchange', changed);
    addEventListener('keydown', esc);
    return () => {
      document.removeEventListener('fullscreenchange', changed);
      removeEventListener('keydown', esc);
      if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    };
  });
  // Left with the Map (another view, the panel closed): no longer over everything.
  $effect(() => () => (mapFull.on = false));

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
    <div class="stage" class:full={mapFull.on} {@attach fill} style:--map-h={mapFull.on ? '100dvh' : mapH ? `${mapH}px` : null}>
      <div class="box">
        {#key insights.removed.join('/')}
          <ProximityGraph
            data={shown}
            signals={prefs.signals}
            bind:selected
            bind:groups
            territories={drawn}
            {solo}
            onmenu={(i, anchor) => (menu = { name: shown.playlists[i].name, anchor })}
          />
        {/key}
        {#if me}
          <button class="corner" onclick={() => viewOver(me.name)}><Icon name="squareArrowOutUpRight" size={18} /><span>View {me.name}</span></button>
        {/if}
        <!-- The legend explains the graph: always at hand, in the graph's corner. -->
        <button class="corner legend-btn" onclick={() => (legendOpen = true)}><Icon name="info" size={18} /><span>Legend</span></button>
        <!-- Genres, bottom left: its sheet (lit while the territories show). -->
        <button class="corner genres-btn" class:on={prefs.mapTerritories} onclick={() => (genresOpen = true)}
          ><Icon name="globe" size={18} /><span>{solo ?? 'Genres'}</span></button
        >
        {#if mapFull.on}
          <button class="corner exit-full" aria-label="Leave full screen" title="Leave full screen" onclick={() => (mapFull.on = false)}><Icon name="exitFullscreen" size={18} /></button>
        {/if}
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

{#if genresOpen}
  <MenuSheet label="Genres" title="Genres" onclose={() => (genresOpen = false)}>
    <div class="rows">
      <Group>
        <Item name="Show on the map" toggle={{ checked: prefs.mapTerritories, onchange: (on) => (prefs.mapTerritories = on) }} />
        {#if prefs.mapTerritories}
          <Item name="Shown" value={solo ?? 'All ticked'} onclick={() => (choosingSolo = true)} />
        {/if}
      </Group>
      <Group title="Genres" more={`Up to ${MAX_GENRES}, each in its own colour; its name runs along its borders. A playlist counts toward a genre from a tenth of its music.`}>
        {#each ranking as g (g.genre)}
          {@const on = ticked.includes(g.genre)}
          <Item
            name={g.genre}
            line={plural(g.playlists, 'playlist')}
            swatch={on ? colourOf(g.genre) : null}
            disabled={!on && ticked.length >= MAX_GENRES}
            toggle={{ checked: on, onchange: (v) => tick(g.genre, v) }}
          />
        {/each}
      </Group>
    </div>
  </MenuSheet>
{/if}
{#if choosingSolo}
  <ChoiceSheet
    title="Shown"
    options={[{ value: '', label: 'All ticked' }, ...drawn.map((g) => ({ value: g.genre, label: g.genre }))]}
    value={solo ?? ''}
    onchange={(v) => (solo = v || null)}
    onclose={() => (choosingSolo = false)}
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

  /* Genres, bottom left; lit while the territories show. */
  .corner.genres-btn {
    top: auto;
    right: auto;
    bottom: 10px;
    left: 10px;
    max-width: 45%;
  }

  .corner.genres-btn.on {
    border-color: var(--accent);
  }

  .corner.exit-full {
    right: auto;
    left: 10px;
    padding: 0 10px;
  }

  /* Full screen: over everything, the whole window. */
  .stage.full {
    position: fixed;
    inset: 0;
    z-index: 1000;
    background: var(--bg);
  }

  .stage.full .box {
    width: 100%;
  }

  .stage.full :global(.wrap) {
    border: 0;
    border-radius: 0;
  }

  .rows {
    padding: 0 16px 16px;
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
