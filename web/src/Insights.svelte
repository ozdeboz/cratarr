<script lang="ts">
  import { ApiError, getInsights, type Count, type Insights, type Proximity, type Signal } from './lib/api';
  import { live } from './lib/live.svelte';
  import Icon from './lib/Icon.svelte';
  import PlaylistMenu from './lib/PlaylistMenu.svelte';
  import { prefs } from './lib/prefs.svelte';
  import ProximityGraph from './lib/ProximityGraph.svelte';
  import { focusRanks, key, rankers, weigh } from './lib/proximity';

  let data = $state<Insights | null>(null);
  let error = $state<string | null>(null);
  let selected = $state<number | null>(null);
  // The playlist menu, opened by a long-press on a point; `name` is the playlist's id.
  let menu = $state<{ name: string; anchor: DOMRect } | null>(null);
  // Deleted here (and not undone): left out of the graph until the next visit.
  let removed = $state<string[]>([]);

  /** The proximity data without deleted playlists, links renumbered. */
  const shown = $derived.by((): Proximity | null => {
    if (!data) return null;
    const p = data.proximity;
    const index = new Map<number, number>();
    p.playlists.forEach((x, i) => !removed.includes(x.name) && index.set(i, index.size));
    const playlists = p.playlists.filter((_, i) => index.has(i));
    const links = p.links
      .filter((l) => index.has(l.a) && index.has(l.b))
      .map((l) => ({ ...l, a: index.get(l.a)!, b: index.get(l.b)! }));
    return { playlists, links };
  });

  /** What the menu shows for a playlist: the live counts when the shell has them. */
  const menuPlaylist = (name: string) => {
    const known = live.view?.playlists.find((x) => x.id === name);
    const point = data?.proximity.playlists.find((x) => x.name === name);
    return known ?? { id: name, name, count: point?.tracks ?? 0, dead: 0 };
  };

  function renamed(from: string, to: string) {
    const point = data?.proximity.playlists.find((x) => x.name === from);
    if (point) point.name = to;
  }

  // The playlist panel (Related only): it follows the selection while open, and has its own address so Back closes
  // it and a link opens it. Beside the graph where there is room, over it where there is not; the graph keeps its size.
  let panelOpen = $state(false);
  // The legend (Related, nothing selected): the same button and panel place; no address of its own.
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
  // Tapping a playlist while the legend is open shows that playlist instead.
  $effect(() => {
    if (legendOpen && me) {
      legendOpen = false;
      openPanel();
    }
  });
  $effect(() => {
    if (!legendOpen) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && !menu && (legendOpen = false);
    addEventListener('keydown', esc);
    return () => removeEventListener('keydown', esc);
  });
  /** Opened in this visit by pushing an address: closing goes Back, so the history stays tidy. */
  let pushed = false;
  const wanted = () => {
    const q = location.hash.split('?')[1];
    return q ? new URLSearchParams(q).get('playlist') : null;
  };
  const addressOf = (name: string) => `#/insights?playlist=${encodeURIComponent(name)}`;
  const me = $derived(shown && selected !== null ? (shown.playlists[selected] ?? null) : null);

  function openPanel() {
    if (!me) return;
    panelOpen = true;
    pushed = true;
    location.hash = addressOf(me.name);
  }

  function closePanel() {
    panelOpen = false;
    if (pushed) {
      pushed = false;
      history.back();
    } else {
      history.replaceState(null, '', '#/insights');
    }
  }

  // Back (or an edited address) decides whether the panel is open.
  $effect(() => {
    const on = () => {
      if (!wanted()) {
        panelOpen = false;
        pushed = false;
      }
    };
    addEventListener('hashchange', on);
    return () => removeEventListener('hashchange', on);
  });

  // A link to a playlist: select it and open the panel once the data is there.
  $effect(() => {
    const name = wanted();
    if (!shown || !name || panelOpen) return;
    const i = shown.playlists.findIndex((p) => p.name === name);
    if (i >= 0) {
      prefs.insightsTab = 'related';
      selected = i;
      panelOpen = true;
    }
  });

  // While open, the panel follows the selection (and its address with it); nothing selected closes it.
  $effect(() => {
    if (!panelOpen) return;
    if (!me) closePanel();
    else if (wanted() !== me.name) history.replaceState(null, '', addressOf(me.name));
  });

  $effect(() => {
    if (!panelOpen) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && !menu && closePanel();
    addEventListener('keydown', esc);
    return () => removeEventListener('keydown', esc);
  });

  $effect(() => {
    getInsights()
      .then((d) => (data = d))
      .catch((e) => (error = e instanceof ApiError ? e.message : 'Unexpected error'));
  });

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

  const num = (n: number) => n.toLocaleString();
  const plural = (n: number, one: string, many = `${one}s`) => `${num(n)} ${n === 1 ? one : many}`;
  const hours = (ms: number) => plural(Math.round(ms / 3_600_000), 'hour');
  const size = (bytes: number) => (bytes >= 1e12 ? `${(bytes / 1e12).toFixed(1)} TB` : `${Math.round(bytes / 1e9)} GB`);
  const pct = (part: number, whole: number) => (whole ? `${Math.round((100 * part) / whole)}%` : '0%');
  const widest = (rows: Count[]) => Math.max(1, ...rows.map((r) => r.n));

  /** The selected playlist's closest playlists under the current signals (scored as the graph does), with what they share. */
  const rank = $derived(shown ? rankers(shown) : null);
  const focus = $derived(shown ? focusRanks(shown) : []);
  const weights = $derived(shown && rank ? weigh(shown, rank, prefs.signals) : new Map<string, number>());
  const closest = $derived.by(() => {
    if (!shown || selected === null) return [];
    return shown.links
      .filter((l) => l.a === selected || l.b === selected)
      .map((l) => ({ l, other: l.a === selected ? l.b : l.a, w: weights.get(key(l.a, l.b)) ?? 0 }))
      .filter((x) => x.w > 0)
      .sort((a, b) => b.w - a.w)
      .slice(0, 8);
  });
</script>

<main class:wide={prefs.insightsTab === 'related'}>
  <div class="toolbar">
    <div class="seg" role="group" aria-label="View">
      {#each TABS as t (t.id)}
        <button aria-pressed={prefs.insightsTab === t.id} onclick={() => (prefs.insightsTab = t.id)}>{t.label}</button>
      {/each}
    </div>
    {#if prefs.insightsTab === 'related'}
      <div class="switches" role="group" aria-label="What brings playlists together">
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
        {#key removed.join('/')}
          <ProximityGraph
            data={shown}
            signals={prefs.signals}
            bind:selected
            bind:groups
            onmenu={(i, anchor) => (menu = { name: shown.playlists[i].name, anchor })}
          />
        {/key}
        {#if me && !panelOpen}
          <button class="open" onclick={openPanel}><Icon name="panel" size={18} /><span>{me.name}</span></button>
        {:else if !me && !legendOpen}
          <button class="open" onclick={() => (legendOpen = true)}><Icon name="panel" size={18} /><span>Legend</span></button>
        {/if}
        {#if !me && legendOpen}
          <aside class="panel" aria-label="Legend">
            <header>
              <h2>Legend</h2>
              <button class="icon" aria-label="Close" onclick={() => (legendOpen = false)}><Icon name="close" size={20} /></button>
            </header>
            <div class="body">
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
          </aside>
        {/if}
        {#if me && panelOpen}
          <aside class="panel" aria-label={me.name}>
            <header>
              <h2>{me.name}</h2>
              <button
                class="icon"
                aria-label={`Menu of ${me.name}`}
                onclick={(e) => (menu = { name: me.name, anchor: e.currentTarget.getBoundingClientRect() })}
                ><Icon name="more" size={20} /></button
              >
              <button class="icon" aria-label="Close" onclick={closePanel}><Icon name="close" size={20} /></button>
            </header>
            <div class="body">
              <p class="muted">{plural(me.tracks, 'track')}{#if me.genres.length} · {me.genres.join(', ')}{/if}</p>
              {#if me.focus !== null && selected !== null}
                <p class="muted">More focused than {Math.round(100 * focus[selected])}% of your playlists</p>
              {/if}
              <h3>Closest playlists</h3>
              {#if closest.length}
                <ul class="plain">
                  {#each closest as c (c.other)}
                    <li>
                      <button class="link" onclick={() => (selected = c.other)}>{shown.playlists[c.other].name}</button>
                      <span class="muted">
                        {[
                          c.l.shared_tracks ? plural(c.l.shared_tracks, 'shared track') : null,
                          c.l.genres.length ? c.l.genres.join(', ') : null,
                          c.l.shared_artists ? plural(c.l.shared_artists, 'shared artist') : null,
                        ]
                          .filter(Boolean)
                          .join(' · ')}
                      </span>
                    </li>
                  {/each}
                </ul>
              {:else}
                <p class="muted">Not close to another playlist with these switches.</p>
              {/if}
            </div>
          </aside>
        {/if}
      </div>
    </div>
    {#if !me && !legendOpen}
      <p class="hint">Tap a playlist to highlight its closest playlists. Pinch or scroll to zoom.</p>
    {/if}
  {/if}
</main>

{#if menu}
  <PlaylistMenu
    playlist={menuPlaylist(menu.name)}
    anchor={menu.anchor}
    onclose={() => (menu = null)}
    onrenamed={renamed}
    ondeleted={(id) => {
      selected = null;
      removed = [...removed, id];
    }}
    onrestored={(id) => (removed = removed.filter((x) => x !== id))}
  />
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

  .open span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(360px, 100%);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: -8px 0 24px color-mix(in srgb, #000 25%, transparent);
    overflow: hidden;
  }

  @container (min-width: 1276px) {
    .panel {
      right: auto;
      left: calc(100% + 16px);
      box-shadow: none;
    }
  }

  .panel header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 6px 6px 14px;
    border-bottom: 1px solid var(--border);
  }

  .panel header h2 {
    flex: 1;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 40px;
    min-height: 40px;
    padding: 0;
    background: none;
    border-radius: 10px;
    color: var(--muted);
  }

  .icon:hover,
  .icon:focus-visible {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .panel .body {
    flex: 1;
    overflow-y: auto;
    padding: 4px 14px 16px;
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
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
    margin-bottom: 8px;
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

  .switches {
    display: flex;
    flex-wrap: wrap;
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

  .panel .plain li {
    justify-content: flex-start;
    flex-wrap: wrap;
  }

  button.link {
    min-height: 32px;
    padding: 0;
    background: none;
    color: var(--accent);
    font-weight: 600;
  }
</style>
