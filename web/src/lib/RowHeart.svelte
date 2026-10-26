<script lang="ts">
  // Finamp's heart on a list's row (decision 0023): filled for a favourite; a tap opens the track's playlists, a
  // long-press (right-click) opens the heart's sheet (the owner, 2026-10-19, lib/HeartSheet): Favourite, the Excludes
  // that apply where the row is (`outs`: a crate's), and Exclude globally.
  import type { ExcludeEntry } from './crates.svelte';
  import { flipFavorite, isFavorite } from './favourites.svelte';
  import HeartSheet from './HeartSheet.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import { BANDCAMP } from './bandcampPreview';

  let {
    source,
    id,
    name,
    artist = null,
    cover = null,
    outs = [],
    onopen,
  }: { source: string; id: string; name: string; artist?: string | null; cover?: string | null; outs?: ExcludeEntry[]; onopen: () => void } = $props();
  let sheet = $state(false);
</script>

<!-- A Bandcamp track has no heart here (the owner, 2026-10-26): crossed out, said so. -->
{#if source === BANDCAMP}
  <span class="rheart off" role="img" aria-label="On Bandcamp" title="On Bandcamp"><Icon name="heartOff" size={24} /></span>
{:else}
<button
  class="rheart"
  class:on={isFavorite(source, id)}
  aria-label={`Add ${name} to a playlist`}
  onclick={() => (navigator.vibrate?.(8), onopen())}
  use:longpress={() => (navigator.vibrate?.(8), (sheet = true))}><Icon name="heart" size={24} /></button
>
{/if}
{#if sheet}
  <HeartSheet
    track={{ source, id, name, artist }}
    {cover}
    favourite={{ on: isFavorite(source, id), flip: () => flipFavorite(source, id) }}
    {outs}
    onclose={() => (sheet = false)}
  />
{/if}

<style>
  .rheart {
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--art-accent, var(--accent));
    cursor: pointer;
  }

  .rheart.on :global(svg) {
    fill: currentColor;
  }

  .rheart.off {
    color: var(--muted);
    cursor: default;
  }
</style>
