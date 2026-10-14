<script lang="ts">
  // Finamp's heart on a list's row (decision 0023): filled for a favourite; a tap opens the track's playlists, a
  // long-press (right-click) makes it a favourite or not.
  import { flipFavorite, isFavorite } from './favourites.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';

  let { source, id, name, onopen }: { source: string; id: string; name: string; onopen: () => void } = $props();
</script>

<button
  class="rheart"
  class:on={isFavorite(source, id)}
  aria-label={`Add ${name} to a playlist`}
  onclick={() => (navigator.vibrate?.(8), onopen())}
  use:longpress={() => flipFavorite(source, id)}><Icon name="heart" size={24} /></button
>

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
</style>
