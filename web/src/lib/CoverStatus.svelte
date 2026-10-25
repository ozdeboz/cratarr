<script lang="ts">
  // An album not in the library that was asked for (the owner, 2026-10-25): on its cover, top left (Play keeps the top
  // right), outline only, so it reads as where it stands, not as a button. Wanted: a clock in the ring; downloading: an
  // arrow in the progress ring, how far. Nothing once it is in the library. What holds it is positioned.
  import type { RequestMark } from './api';
  import Icon from './Icon.svelte';
  import ProgressRing from './ProgressRing.svelte';

  let { mark, small = false }: { mark: RequestMark | null | undefined; small?: boolean } = $props();
  const size = $derived(small ? 20 : 30);
  const done = $derived(mark?.state === 'downloading' ? (mark.done ?? 0) : null);
</script>

{#if mark}
  <span
    class="cover-status"
    class:small
    role="img"
    aria-label={mark.state === 'wanted' ? 'Requested' : `Downloading, ${Math.round((done ?? 0) * 100)}%`}
    title={mark.state === 'wanted' ? 'Requested' : 'Downloading'}
    style:width="{size}px"
    style:height="{size}px"
    ><ProgressRing progress={done} size={size} /><Icon name={mark.state === 'wanted' ? 'clock' : 'arrowDown'} size={small ? 12 : 16} /></span
  >
{/if}

<style>
  .cover-status {
    position: absolute;
    top: 6px;
    left: 6px;
    z-index: 1;
    display: grid;
    place-items: center;
    color: var(--accent);
    /* Transparent (the owner's): a faint shadow keeps the outline readable on any cover. */
    filter: drop-shadow(0 0 2px rgb(0 0 0 / 0.55));
    pointer-events: none;
  }

  .cover-status.small {
    top: 2px;
    left: 2px;
  }
</style>
