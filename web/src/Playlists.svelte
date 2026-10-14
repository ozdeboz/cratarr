<script lang="ts">
  import { tick as settled } from 'svelte';
  import { createPlaylist, type Playlist } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { live, queueTap, refresh } from './lib/live.svelte';
  import Notice from './lib/Notice.svelte';
  import { toast } from './lib/toast.svelte';
  import { withUndo } from './lib/undo.svelte';
  import { followCreate, followDelete } from './lib/follow.svelte';
  import NewButton from './lib/NewButton.svelte';
  import PlaylistMenu from './lib/PlaylistMenu.svelte';
  import NameSheet from './lib/NameSheet.svelte';
  import Icon from './lib/Icon.svelte';
  import Seg from './lib/Seg.svelte';
  import SizeControl from './lib/SizeControl.svelte';
  import { isLocal, setPlayingMember, shownPlaylists } from './lib/output.svelte';
  import { pending, playing } from './lib/remote.svelte';
  import { longpress } from './lib/longpress';
  import { fromAction } from 'svelte/attachments';
  import PlaylistItem from './lib/PlaylistItem.svelte';
  import { viewPlaylist, panel } from './lib/panel.svelte';
  import { isPlaylistFavorite, loadPlaylistFavorites } from './lib/favourites.svelte';
  import { endSelect, viewPicked, select, showsPicked, startSelect, tick } from './lib/select.svelte';

  // What is playing and the playlists come from the shell's poll (live.svelte.ts).
  const view = $derived(live.view);
  // The favourites' hearts on the chips (decision 0023).
  $effect(() => void loadPlaylistFavorites());
  const error = $derived(live.error);
  let filter = $state('');
  // On a narrow page the filter field starts as a magnifier; open, it lies over the rest of its row (Esc, ✕, or
  // leaving it empty folds it again).
  let searching = $state(false);
  let searchField = $state<HTMLInputElement>();
  async function openSearch() {
    searching = true;
    await settled();
    searchField?.focus();
  }
  /** The filter section's Favourites toggle: only favourite playlists. */
  let favouritesOnly = $state(false);
  // Size = chip width, which sets how many columns fit; one width per step in prefs.ZOOM_STEPS.
  const WIDTHS = [110, 140, 170, 210, 260, 340];
  let gridWidth = $state(0);
  // Taps flip the chip at once; the server's answer replaces this guess.
  let optimistic = $state<Record<string, boolean>>({});
  let busy = $state<Record<string, boolean>>({});

  // The open menu (a sheet, decision 0021): a playlist's (lib/PlaylistMenu), the "New playlist" one, or the ticked
  // playlists' (Select's ⋮, decision 0025).
  type Menu = { kind: 'playlist'; p: Playlist; anchor: DOMRect } | { kind: 'new'; anchor: DOMRect } | { kind: 'selection' };
  let menu = $state<Menu | null>(null);
  let working = $state(false);

  const np = $derived(playing());
  const items = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    let list = shownPlaylists().map((p) => {
      const member = optimistic[p.id] ?? p.member;
      return { ...p, member, count: p.count + (member === p.member ? 0 : member ? 1 : -1) };
    });
    if (q) list = list.filter((p) => p.name.toLowerCase().includes(q));
    if (favouritesOnly) list = list.filter((p) => isPlaylistFavorite(p.id));
    // Pinned ones first, in the order pinned; "✓ first" applies within the pinned and within the rest.
    const pin = (id: string) => {
      const i = prefs.pinned.indexOf(id);
      return i < 0 ? Infinity : i;
    };
    return [...list].sort((a, b) => {
      const [pa, pb] = [pin(a.id), pin(b.id)];
      if ((pa === Infinity) !== (pb === Infinity)) return pa === Infinity ? 1 : -1;
      if (prefs.membersFirst && a.member !== b.member) return Number(b.member) - Number(a.member);
      return pa === Infinity ? 0 : pa - pb;
    });
  });

  const memberCount = $derived(shownPlaylists().filter((p) => optimistic[p.id] ?? p.member).length);
  const NOT_MUSIC = "Not in your Music, so it can't go in playlists.";

  async function tap(p: Playlist) {
    if (busy[p.id]) return;
    if (!np) {
      toast.show({ text: 'Nothing is playing. Start a track to add it to playlists.', tone: 'warn' });
      return;
    }
    if (!np.addable && !p.member) {
      toast.show({ text: NOT_MUSIC, tone: 'warn' });
      return;
    }
    // The server's player is moving to another track: the tap waits for it (live.svelte.ts). The local track's
    // playlists are a moment away: the tap is not taken.
    if (pending()) {
      if (!isLocal()) queueTap(p.id);
      return;
    }
    busy[p.id] = true;
    optimistic[p.id] = !p.member;
    try {
      await setPlayingMember(p.id, !p.member);
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      delete optimistic[p.id];
      delete busy[p.id];
    }
  }

  const missing = (n: number) => `${n} missing ${n === 1 ? 'track' : 'tracks'}`;

  async function create(name: string) {
    if (working) return;
    working = true;
    try {
      const { undo } = await createPlaylist(name);
      withUndo(`Created ${name}.`, [undo], { after: (undone) => (undone ? followDelete(name) : followCreate(name)) });
      menu = null;
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      working = false;
      await refresh();
    }
  }

  function openNew(e: MouseEvent) {
    menu = { kind: 'new', anchor: (e.currentTarget as HTMLElement).getBoundingClientRect() };
  }

</script>

<main class:ticked={select.on && select.picked.length > 0}>
  <!-- The page's top right corner: New playlist (+), or with playlists ticked their menu (⋮, decision 0025). -->
  {#if select.on && select.picked.length}
    <button class="new corner" aria-label="Menu of the ticked playlists" onclick={() => (menu = { kind: 'selection' })}
      ><Icon name="moreVert" size={22} /></button
    >
  {:else}
    <NewButton label="New playlist" onclick={openNew} />
  {/if}

  <!-- The page's controls in two sections: what it always needs (the button that shows or hides the other section,
       Select, Size), and the filter section (the filter and the order), which folds away; remembered by the browser. -->
  <div class="tools">
    <div class="row">
    <button
      class="new toggle"
      class:active={!prefs.filterOpen && (filter.trim() !== '' || favouritesOnly)}
      aria-expanded={prefs.filterOpen}
      aria-controls="playlist-filter"
      aria-label={prefs.filterOpen ? 'Hide the filter' : 'Show the filter'}
      onclick={() => (prefs.filterOpen = !prefs.filterOpen)}><Icon name="filter" size={22} /></button
    >
    <!-- Select (decision 0025): on, it is highlighted, a chip's name ticks it, and a tap on it again ends it. The ticked
         ones' menu is the ⋮ in the page's top right corner; on a phone they are viewed in the panel with View N. -->
    <button
      class="new toggle select"
      aria-pressed={select.on}
      aria-label="Select"
      title="Select"
      onclick={() => (select.on ? endSelect() : startSelect())}><Icon name="copyCheck" size={22} /></button
    >
    <SizeControl widths={WIDTHS} width={gridWidth} bind:value={prefs.zoom} />
    {#if select.on && select.picked.length && !showsPicked()}
      <!-- View N: at the right end of the row (beside the corner); on a small page its icon only. -->
      {@const viewLabel = select.picked.length === 1 ? 'View 1 playlist' : `View ${select.picked.length} playlists`}
      <button class="new primary view-picked" aria-label={viewLabel} title={viewLabel} onclick={viewPicked}
        ><Icon name="playlists" size={22} /><span class="view-text">{viewLabel}</span></button
      >
    {/if}
    </div>
    {#if prefs.filterOpen}
      <div class="row filters" id="playlist-filter">
        <button class="new toggle magnifier" class:active={filter.trim() !== ''} aria-label="Filter playlists" aria-expanded={searching} onclick={openSearch}
          ><Icon name="search" size={22} /></button
        >
        <div class="search" class:open={searching}>
          <input
            type="search"
            placeholder="Filter playlists"
            aria-label="Filter playlists"
            bind:value={filter}
            bind:this={searchField}
            onkeydown={(e) => e.key === 'Escape' && (searching = false)}
            onblur={() => filter.trim() === '' && (searching = false)}
          />
          <button
            class="clear"
            aria-label="Clear the filter"
            onmousedown={(e) => e.preventDefault()}
            onclick={() => {
              filter = '';
              searching = false;
            }}><Icon name="close" size={20} /></button
          >
        </div>
        <Seg
          label="Order"
          options={[
            { value: false, label: 'A–Z' },
            { value: true, label: '✓ first', title: 'Playlists with the playing track first' },
          ]}
          bind:value={prefs.membersFirst}
        />
        <button
          class="new toggle fav"
          aria-pressed={favouritesOnly}
          aria-label="Favourites only"
          title="Favourites only"
          onclick={() => (favouritesOnly = !favouritesOnly)}><Icon name="heart" size={22} /></button
        >
      </div>
    {/if}
  </div>

  {#if error}
    <Notice text={error} />
  {/if}

  {#if !view && !error}
    <p class="empty">Loading…</p>
  {:else}
    <ul class="grid" class:pending={pending()} style:--min="{WIDTHS[prefs.zoom]}px" bind:clientWidth={gridWidth}>
      {#each items as p (p.id)}
        <li>
          <!-- The label views the playlist (decision 0023), in Select ticks it (decision 0025); the circle adds or removes
               the playing track. The border: in the panel's selection (or ticked). -->
          <PlaylistItem
            {p}
            size="medium"
            class={['chip', busy[p.id] && 'busy', live.queued.includes(p.id) && 'queued', !np && 'idle', !!np && !np.addable && !p.member && 'blocked']
              .filter(Boolean)
              .join(' ')}
            strong={p.member}
            active={select.on ? select.picked.includes(p.id) : panel.selection.includes(p.id)}
            label={select.on ? `${select.picked.includes(p.id) ? 'Untick' : 'Tick'} ${p.name}` : `View ${p.name}`}
            onclick={() => (select.on ? tick(p.id) : viewPlaylist(p.id))}
            attach={fromAction(longpress, () => (anchor: DOMRect) => (menu = { kind: 'playlist', p, anchor }))}
          >
            {#snippet action()}
              <button
                class="icon"
                aria-pressed={p.member}
                aria-label={p.member ? `Take the playing track out of ${p.name}` : `Add the playing track to ${p.name}`}
                onclick={() => tap(p)}
                ><Icon name={p.member ? 'circleCheck' : np && !np.addable ? 'circleDashedPlus' : 'circlePlus'} size={26} /></button
              >
            {/snippet}
          </PlaylistItem>
        </li>
      {/each}
    </ul>
    {#if view && items.length === 0}<p class="empty">No playlists match.</p>{/if}
  {/if}
</main>

{#if menu?.kind === 'new'}
  <NameSheet title="New playlist" what="playlist" action="Create playlist" onsubmit={create} onclose={() => (menu = null)} />
{:else if menu?.kind === 'playlist'}
  <PlaylistMenu playlists={[menu.p]} onclose={() => (menu = null)} />
{:else if menu?.kind === 'selection'}
  <!-- The same menu as a long-press, for what is ticked. -->
  <PlaylistMenu playlists={select.picked.flatMap((id) => shownPlaylists().filter((x) => x.id === id))} onclose={() => (menu = null)} />
{/if}

<style>
  /* The page answers to its own width (beside the player or not), not the window's: a container. */
  main {
    position: relative;
    padding: 14px clamp(12px, 2vw, 32px) 48px;
    container-type: inline-size;
  }

  /* The corner's + (with its label on a wide page) or ⋮: the actions keep clear of it. */
  .tools > .row:first-child {
    padding-right: 56px;
  }

  @container (min-width: 400px) {
    .tools > .row:first-child {
      padding-right: 170px;
    }

    /* With playlists ticked the corner is the ⋮, without a label. */
    main.ticked .tools > .row:first-child {
      padding-right: 56px;
    }
  }

  .tools {
    display: grid;
    gap: 12px;
    margin: 0 0 14px;
  }

  .tools .row {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  button.new.toggle {
    display: grid;
    place-items: center;
    width: 44px;
    padding: 0;
    color: inherit;
  }

  /* Favourites only and Select: on, in the accent (the heart filled). */
  button.new.select[aria-pressed='true'],
  button.new.fav[aria-pressed='true'] {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
  }

  button.new.fav[aria-pressed='true'] :global(svg) {
    fill: currentColor;
  }

  /* Folded away while a filter applies: the button says so, since chips are missing. */
  button.new.toggle.active {
    border-color: var(--accent);
    color: var(--accent);
  }

  input[type='search'] {
    min-height: 44px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  input[type='search'] {
    flex: 1 1 220px;
    min-width: 0;
    background: var(--surface);
  }

  /* The filter field: in its row while the page is wide enough for the row on one line (the field at its 220 px
     beside the order and the heart); narrower, a magnifier, and once opened lying over the row. */
  /* The row never wraps: the field takes what the order and the heart leave (an input's own minimum width would push
     them to a line of their own), and below the container query's width it folds to the magnifier. */
  .tools .row.filters {
    position: relative;
    flex-wrap: nowrap;
  }

  .search {
    display: flex;
    flex: 1 1 0;
    min-width: 0;
  }

  .search input {
    flex: 1 1 0;
    width: 100%;
    min-width: 0;
  }

  button.new.magnifier,
  .search .clear {
    display: none;
  }

  @container (max-width: 439px) {
    button.new.magnifier {
      display: grid;
    }

    .search {
      display: none;
    }

    .search.open {
      position: absolute;
      inset: 0;
      z-index: 1;
      display: flex;
      align-items: center;
      background: var(--bg);
    }

    .search.open input {
      align-self: stretch;
      padding-right: 48px;
    }

    /* One ✕: ours (it also folds the field), not the browser's. */
    .search.open input::-webkit-search-cancel-button {
      display: none;
    }

    .search.open .clear {
      position: absolute;
      right: 0;
      display: grid;
      place-items: center;
      width: 44px;
      height: 44px;
      padding: 0;
      border: 0;
      background: none;
      color: inherit;
      cursor: pointer;
    }
  }

  button.new {
    min-height: 44px;
    padding: 0 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    cursor: pointer;
  }

  button.new.corner {
    position: absolute;
    top: 14px;
    right: clamp(12px, 2vw, 32px);
    display: grid;
    place-items: center;
    width: 44px;
    padding: 0;
    color: inherit;
  }

  .view-picked {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    padding: 0 10px;
  }

  .view-picked .view-text {
    display: none;
  }

  @container (min-width: 480px) {
    .view-picked {
      padding: 0 14px 0 10px;
    }

    .view-picked .view-text {
      display: inline;
    }
  }

  button.new.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  button.new:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--min, 170px), 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* The chips are medium rows (lib/Row.svelte, Finamp's add-to-playlist tile); what is the page's own is how adding
     the playing track stands. Nothing playing: the chips only view, so they look quieter. */
  .grid :global(.chip.idle) {
    opacity: 0.6;
  }

  .grid :global(.chip.busy) {
    opacity: 0.7;
  }

  /* The player is moving to another track: its playlists are not known yet, so the chips are dimmed, and a tapped one
     pulses until the tap goes through (live.svelte.ts). */
  .grid.pending :global(.chip:not(.queued)) {
    opacity: 0.45;
  }

  .grid :global(.chip.queued) {
    outline: 1.5px solid var(--accent);
    animation: waiting 1s ease-in-out infinite alternate;
  }

  @keyframes waiting {
    from {
      opacity: 1;
    }
    to {
      opacity: 0.55;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .grid :global(.chip.queued) {
      animation: none;
    }
  }

  /* Track outside Music: adding is refused, so the add chips are quieter and their plus circle dashed (Finamp's mark
     for a playlist it cannot add to); tapping explains why. */
  .grid :global(.chip.blocked) {
    opacity: 0.6;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }

</style>
