<script lang="ts">
  // A menu's playlists as toggle cards (Add to playlist: a track's, an album's, an artist's) with the standard size
  // buttons (lib/SizeControl, decided 2026-10-16: "so that on mobile we can get two columns"): the cards in columns,
  // as many as the size gives (LIST_WIDTHS: never three on a phone); a name wraps to two lines. A button that would
  // change nothing is dimmed. The size is remembered (prefs.listZoom), apart from the
  // Playlists page's.
  import type { Snippet } from 'svelte';
  import { LIST_WIDTHS } from './pageView.svelte';
  import { prefs } from './prefs.svelte';
  import SizeControl from './SizeControl.svelte';

  let { title, children }: { title: string; children: Snippet } = $props();
  let width = $state(0);
</script>

<div class="head">
  <span class="heading">{title}</span>
  <SizeControl plain widths={LIST_WIDTHS} {width} bind:value={prefs.listZoom} />
</div>
<div class="grid" role="group" aria-label="Playlists" bind:clientWidth={width} style:--least="{LIST_WIDTHS[prefs.listZoom]}px">
  {@render children()}
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 8px 8px 16px;
  }

  .heading {
    min-width: 0;
    font-size: 18px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, var(--least)), 1fr));
    gap: 8px;
    padding: 0 12px 4px;
  }

  .grid > :global(.toggle) {
    width: 100%;
    height: 100%;
    margin: 0;
  }

  .grid > :global(.toggle span) {
    display: -webkit-box;
    white-space: normal;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
</style>
