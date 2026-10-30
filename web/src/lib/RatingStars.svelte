<script lang="ts">
  // An album's rating as five stars (Symfonium's "Change user rating"), filled up to it in the accent: a tap rates, a
  // tap on the rating it has takes it off (lib/albums rateAlbum). The album's menu and its About show the same.
  import Icon from './Icon.svelte';

  let { value, onrate, size = 24 }: { value: number; onrate: (stars: number) => void; size?: number } = $props();
</script>

<span class="stars" role="group" aria-label="Rating">
  {#each [1, 2, 3, 4, 5] as n (n)}
    <button class:on={n <= value} aria-label={n === 1 ? '1 star' : `${n} stars`} aria-pressed={n === value} onclick={() => onrate(n)}
      ><Icon name="star" {size} /></button
    >
  {/each}
</span>

<style>
  .stars {
    display: flex;
  }

  button {
    display: grid;
    place-items: center;
    width: 40px;
    height: 44px;
    padding: 0;
    background: none;
    border: 0;
    color: color-mix(in srgb, var(--text) 45%, transparent);
    cursor: pointer;
  }

  .on {
    color: var(--art-accent, var(--accent));
  }

  .on :global(svg) {
    fill: currentColor;
  }
</style>
