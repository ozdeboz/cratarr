<script lang="ts">
  // Symfonium's floating pill (2026-10-15), at the top right of the panel (lib/PlaylistView) and of a page with something
  // to play (App.svelte): the filter (lib/FilterSheet), a page's View (lib/ViewSheet: its order, layout and size), and
  // where it plays (lib/OutputSheet) where nothing else leads to it (no mini-player, no player beside the page). Google's
  // Cast guidelines put the device button at the top right of the screen that plays.
  import FilterSheet from './FilterSheet.svelte';
  import Icon from './Icon.svelte';
  import OutputSheet from './OutputSheet.svelte';
  import ViewSheet from './ViewSheet.svelte';

  let {
    page = null,
    view = false,
    device = true,
  }: {
    /** The page it is on (its own filters and View), or null in the panel. */
    page?: string | null;
    view?: boolean;
    device?: boolean;
  } = $props();
  let filterOpen = $state(false);
  let viewOpen = $state(false);
  let outputOpen = $state(false);
</script>

<div class="pill" role="toolbar" aria-label="Filter and where it plays">
  <button aria-label="Filter" title="Filter" aria-haspopup="dialog" onclick={() => (filterOpen = true)}><Icon name="funnel" size={22} /></button>
  {#if view && page}<button aria-label="View" title="View" aria-haspopup="dialog" onclick={() => (viewOpen = true)}><Icon name="sliders" size={22} /></button>{/if}
  {#if device}<button aria-label="Where it plays" title="Where it plays" aria-haspopup="dialog" onclick={() => (outputOpen = true)}><Icon name="speaker" size={22} /></button>{/if}
</div>
{#if filterOpen}
  <FilterSheet {page} onclose={() => (filterOpen = false)} />
{/if}
{#if viewOpen && page}
  <ViewSheet {page} onclose={() => (viewOpen = false)} />
{/if}
{#if outputOpen}
  <OutputSheet onclose={() => (outputOpen = false)} />
{/if}

<style>
  /* A dark round-ended pill as Symfonium's, a little see-through so a cover under it still shows. */
  .pill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: color-mix(in srgb, var(--surface) 80%, transparent);
    backdrop-filter: blur(12px);
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
</style>
