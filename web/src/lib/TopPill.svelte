<script lang="ts">
  // Symfonium's floating pill (2026-10-15), at the top right of the panel (lib/PlaylistView) and of a page with something
  // to play (App.svelte): the page's search (it opens into a field over the row), the filter (lib/FilterSheet), View
  // (lib/ViewSheet: a page's order, layout and size, or the panel list's order), and where it plays (lib/OutputSheet)
  // where nothing else leads to it (no mini-player, no player beside the page), and last the page's own button (a + to
  // make something, its ticked ones' ⋮; pageView.usePageAction). Google's Cast guidelines put the device button at the
  // top right of the screen that plays.
  import FilterSheet from './FilterSheet.svelte';
  import Icon from './Icon.svelte';
  import OutputSheet from './OutputSheet.svelte';
  import { pageView, searchFor } from './pageView.svelte';
  import { panel } from './panel.svelte';
  import ViewSheet, { type ListOrder } from './ViewSheet.svelte';

  let {
    page = null,
    view = false,
    device = true,
    order = null,
  }: {
    /** The page it is on (its search, its own filters and View), or null in the panel. */
    page?: string | null;
    view?: boolean;
    device?: boolean;
    /** The panel list's order, for its View. */
    order?: ListOrder | null;
  } = $props();
  let filterOpen = $state(false);
  let viewOpen = $state(false);
  let outputOpen = $state(false);

  // The search: the magnifier opens a field over the row (the whole row on a phone, the pill grown leftwards on a
  // wide page); Esc or leaving it empty folds it, ✕ empties it. A search that applies keeps the magnifier lit.
  const words = $derived(page ? searchFor(page) : null);
  let searching = $state(false);
  let field = $state<HTMLInputElement>();
  $effect(() => {
    if (searching) field?.focus();
  });
  $effect(() => {
    void page;
    searching = false;
  });
  // A filter leaves something out of what shows: a library switched off (remembered, so it is said here), Playlists'
  // Favourites only, the panel's Hide missing tracks.
  const filtering = $derived(page ? pageView.narrowed || pageView.favouritesOnly || pageView.upgradesOnly : panel.hideMissing);
  function clear() {
    pageView.query = '';
    searching = false;
  }
</script>

{#if searching && words}
  <div class="pill wide" role="search">
    <span class="lead" aria-hidden="true"><Icon name="search" size={20} /></span>
    <input
      bind:this={field}
      type="search"
      placeholder={words}
      aria-label={words}
      bind:value={pageView.query}
      onkeydown={(e) => e.key === 'Escape' && (searching = false)}
      onblur={() => pageView.query.trim() === '' && (searching = false)}
    />
    <button aria-label="Clear the search" title="Clear" onmousedown={(e) => e.preventDefault()} onclick={clear}><Icon name="close" size={20} /></button>
  </div>
{:else}
  <div class="pill" role="toolbar" aria-label="The page's tools">
    {#if words}
      <button class:lit={pageView.query.trim() !== ''} aria-label={words} title={words} onclick={() => (searching = true)}><Icon name="search" size={22} /></button>
    {/if}
    <button class:lit={filtering} aria-label="Filter" title="Filter" aria-haspopup="dialog" onclick={() => (filterOpen = true)}><Icon name="funnel" size={22} /></button>
    {#if view && (page || order)}<button aria-label="View" title="View" aria-haspopup="dialog" onclick={() => (viewOpen = true)}><Icon name="sliders" size={22} /></button>{/if}
    {#if device}<button aria-label="Where it plays" title="Where it plays" aria-haspopup="dialog" onclick={() => (outputOpen = true)}><Icon name="speaker" size={22} /></button>{/if}
    {#if page && pageView.action}
      {@const a = pageView.action}
      <button class="action" aria-label={a.label} title={a.label} onclick={a.run}
        ><Icon name={a.icon} size={22} />{#if a.progress != null}<svg class="ring" viewBox="0 0 36 36" aria-hidden="true"
            ><circle class="bg" cx="18" cy="18" r="16" /><circle cx="18" cy="18" r="16" pathLength="100" stroke-dasharray="{Math.round(
                Math.min(1, Math.max(0, a.progress)) * 100,
              )} 100" /></svg
          >{/if}</button
      >
    {/if}
  </div>
{/if}
{#if filterOpen}
  <FilterSheet {page} onclose={() => (filterOpen = false)} />
{/if}
{#if viewOpen && (page || order)}
  <ViewSheet {page} {order} onclose={() => (viewOpen = false)} />
{/if}
{#if outputOpen}
  <OutputSheet onclose={() => (outputOpen = false)} />
{/if}

<style>
  /* A dark round-ended pill as Symfonium's, a little see-through and blurring what is under it. */
  .pill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 2px 12px rgb(0 0 0 / 0.3);
  }

  button {
    display: grid;
    place-items: center;
    width: 48px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 22px;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  button:hover {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  /* The page's own button with work going on: a ring of its progress around the icon. */
  .action {
    position: relative;
  }

  .ring {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 38px;
    height: 38px;
    fill: none;
    stroke: var(--accent);
    stroke-width: 2.5;
    stroke-linecap: round;
    transform: translate(-50%, -50%) rotate(-90deg);
    pointer-events: none;
  }

  .ring .bg {
    stroke: color-mix(in srgb, var(--accent) 25%, transparent);
  }

  /* A search or a filter that applies. */
  button.lit {
    color: var(--accent);
  }

  /* The search open: over the row (it is the row's, positioned), the whole of it on a phone, at most 520 px. */
  .pill.wide {
    position: absolute;
    top: 0;
    right: 0;
    width: min(100%, 520px);
    box-sizing: border-box;
    align-items: center;
    background: var(--surface);
  }

  .lead {
    display: grid;
    place-items: center;
    width: 40px;
    color: var(--muted);
  }

  .wide input {
    flex: 1;
    min-width: 0;
    height: 44px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 16px;
    outline: none;
  }

  /* One ✕: ours (it also folds the field), not the browser's. */
  .wide input::-webkit-search-cancel-button {
    display: none;
  }
</style>
