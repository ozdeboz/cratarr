<script lang="ts">
  import { untrack } from 'svelte';
  import { type Count, type Signal } from './lib/api';
  import { insights, plural } from './lib/insights.svelte';
  import { live } from './lib/live.svelte';
  import Icon from './lib/Icon.svelte';
  import MenuSheet from './lib/MenuSheet.svelte';
  import { openPlaylist, panel } from './lib/panel.svelte';
  import PlaylistMenu from './lib/PlaylistMenu.svelte';
  import { prefs } from './lib/prefs.svelte';
  import ProximityGraph from './lib/ProximityGraph.svelte';

  // The data and what is derived from it live in a store shared with the playlist view's Related (decision 0023).
  $effect(() => insights.load());
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

  // The opened playlist and the graph's selection are one (decision 0023): opening a playlist selects it here, and
  // selecting another point while one is open shows that one in the panel instead.
  $effect(() => {
    const name = panel.open;
    if (!shown || !name) return;
    const i = insights.indexOf(name);
    if (i >= 0 && untrack(() => selected) !== i) selected = i;
  });
  $effect(() => {
    const i = selected;
    if (i === null || !shown) return;
    const name = shown.playlists[i]?.name;
    const open = untrack(() => panel.open);
    if (open && name && name !== open) openPlaylist(name);
  });

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

  const TABS = [
    { id: 'statistics', label: 'Statistics' },
    { id: 'related', label: 'Related' },
  ] as const;
  const SIGNALS: { id: Signal; label: string }[] = [
    { id: 'tracks', label: 'Shared tracks' },
    { id: 'genre', label: 'Genre' },
    { id: 'artists', label: 'Artists' },
  ];
  const anyOn = $derived(SIGNALS.some((s) => prefs.signals[s.id]));
  /** A signal is off: the filter applies (the folded filter button says so). */
  const filtering = $derived(SIGNALS.some((s) => !prefs.signals[s.id]));

  const num = (n: number) => n.toLocaleString();
  const hours = (ms: number) => plural(Math.round(ms / 3_600_000), 'hour');
  const size = (bytes: number) => (bytes >= 1e12 ? `${(bytes / 1e12).toFixed(1)} TB` : `${Math.round(bytes / 1e9)} GB`);
  const pct = (part: number, whole: number) => (whole ? `${Math.round((100 * part) / whole)}%` : '0%');
  const widest = (rows: Count[]) => Math.max(1, ...rows.map((r) => r.n));

</script>

<main class:wide={prefs.insightsTab === 'related'}>
  <!-- As the Playlists page: the views always, and on Related its filter section (what brings playlists together),
       which the filter button shows or hides; remembered by the browser. -->
  <div class="toolbar">
    <div class="row">
      <div class="seg" role="group" aria-label="View">
        {#each TABS as t (t.id)}
          <button aria-pressed={prefs.insightsTab === t.id} onclick={() => (prefs.insightsTab = t.id)}>{t.label}</button>
        {/each}
      </div>
      {#if prefs.insightsTab === 'related'}
        <button
          class="toggle"
          class:active={!prefs.relatedFilterOpen && filtering}
          aria-expanded={prefs.relatedFilterOpen}
          aria-controls="related-filter"
          aria-label={prefs.relatedFilterOpen ? 'Hide the filter' : 'Show the filter'}
          onclick={() => (prefs.relatedFilterOpen = !prefs.relatedFilterOpen)}><Icon name="filter" size={22} /></button
        >
      {/if}
    </div>
    {#if prefs.insightsTab === 'related' && prefs.relatedFilterOpen}
      <div class="row switches" id="related-filter" role="group" aria-label="What brings playlists together">
        {#each SIGNALS as s (s.id)}
          <button class="chip" aria-pressed={prefs.signals[s.id]} onclick={() => (prefs.signals[s.id] = !prefs.signals[s.id])}>
            {s.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !data}
    <p class="empty">Loading…</p>
  {:else if prefs.insightsTab === 'statistics'}
    {@const l = data.library}
    {@const p = data.playlists}
    <section>
      <h2>Music</h2>
      <p class="figures">
        <span>{plural(l.tracks, 'track')}</span>
        <span>{plural(l.albums, 'album')}</span>
        <span>{plural(l.artists, 'artist')}</span>
        <span>{hours(l.duration_ms)}</span>
        <span>{size(l.bytes)}</span>
      </p>
      <p class="figures">
        <span>{plural(l.lossless_albums, 'lossless album')}</span>
        <span>{plural(l.below_320_albums, 'album')} below 320 kbps</span>
        {#each l.formats as f (f.name)}<span>{num(f.n)} {f.name}</span>{/each}
      </p>
    </section>

    <section>
      <h2>Playlists</h2>
      <p class="figures">
        <span>{plural(p.playlists, 'playlist')}</span>
        <span>{pct(p.library_tracks_in_playlists, l.tracks)} of your tracks are in a playlist</span>
        <span>{plural(p.albums_without_playlist, 'album')} in no playlist</span>
        <span>{plural(p.in_one_playlist, 'track')} in only one</span>
        {#if p.missing}<span>{plural(p.missing, 'missing track')}</span>{/if}
      </p>
      {#if p.most_shared.length}
        <h3>In the most playlists</h3>
        <ol class="plain">
          {#each p.most_shared as t, i (i)}
            <li><span>{t.title}{#if t.artist}<span class="muted"> · {t.artist}</span>{/if}</span><span class="n">in {t.playlists}</span></li>
          {/each}
        </ol>
      {/if}
    </section>

    {#snippet bars(title: string, rows: Count[], unit: string)}
      {#if rows.length}
        <section>
          <h2>{title}</h2>
          <ul class="bars">
            {#each rows as r (r.name)}
              <li>
                <span class="label">{r.name}</span>
                <span class="bar"><i style:width={`${(100 * r.n) / widest(rows)}%`}></i></span>
                <span class="n">{plural(r.n, unit)}</span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/snippet}
    {@render bars('Genres', l.genres, 'album')}
    {@render bars('Decades', l.decades, 'album')}
    {@render bars('Artists', l.top_artists, 'album')}
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
        {#if me && panel.open !== me.name}
          <button class="open" onclick={() => openPlaylist(me.name)}><Icon name="listOpen" size={18} /><span>Open {me.name}</span></button>
        {/if}
        <!-- The legend explains the graph: always at hand, in the graph's corner. -->
        <button class="open legend-btn" onclick={() => (legendOpen = true)}><Icon name="info" size={18} /><span>Legend</span></button>
      </div>
    </div>
    {#if !me}
      <p class="hint">Tap a playlist to highlight its closest playlists. Pinch or scroll to zoom.</p>
    {/if}
  {/if}
</main>

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
  main {
    max-width: 900px;
    padding: 16px clamp(12px, 2vw, 32px) 48px;
  }

  /* Related: room for the panel beside the graph on wide screens. */
  main.wide {
    max-width: none;
  }

  /* The graph keeps its width (at most 900 px). The panel sits beside it when the stage has room for both
     (900 + 16 + 360), else over the graph's right side; full width on a phone. */
  .stage {
    container-type: inline-size;
  }

  .box {
    position: relative;
    width: min(100%, 900px);
  }

  .open {
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
  .open.legend-btn {
    top: auto;
    bottom: 10px;
  }


  .open span {
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

  .toolbar {
    display: grid;
    gap: 10px;
    margin-bottom: 8px;
  }

  .toolbar .row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
  }

  /* The filter button: as the Playlists page's. */
  .toggle {
    display: grid;
    place-items: center;
    width: 44px;
    min-height: 44px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  /* Folded away while a filter applies: the button says so. */
  .toggle.active {
    border-color: var(--accent);
    color: var(--accent);
  }

  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
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

  .seg button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .toolbar .row.switches {
    gap: 6px;
  }

  .chip {
    min-height: 36px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--muted);
  }

  .chip[aria-pressed='true'] {
    border-color: var(--accent);
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 18%, var(--surface));
  }

  h2 {
    margin: 24px 0 8px;
    font-size: 1rem;
  }

  h3 {
    margin: 14px 0 6px;
    font-size: 0.9rem;
    color: var(--muted);
  }

  .figures {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    margin: 0 0 6px;
    font-variant-numeric: tabular-nums;
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
    color: var(--danger);
  }

  .n {
    color: var(--muted);
    font-weight: 400;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .plain {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .plain li {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
    overflow-wrap: anywhere;
  }

  .bars {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 6px;
  }

  .bars li {
    display: grid;
    grid-template-columns: minmax(90px, 30%) 1fr auto;
    align-items: center;
    gap: 10px;
    font-size: 0.9rem;
  }

  .bars .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bar {
    height: 10px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    border-radius: 5px;
    overflow: hidden;
  }

  .bar i {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 5px;
  }
</style>
