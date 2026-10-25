<script lang="ts">
  // An album not in the library that was asked for (the owner, 2026-10-25): on its cover, top left (Play keeps the top
  // right), as Play's round (the owner, 2026-10-25: "the same color style as the play button"; outline only was too
  // faint on covers). Wanted: a clock in the ring; downloading: an
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
    >{#if done !== null}<ProgressRing progress={done} size={size - 6} onAccent />{/if}<Icon name={mark.state === 'wanted' ? 'clock' : 'arrowDown'} size={done !== null ? (small ? 9 : 13) : small ? 12 : 17} /></span
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
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.35);
    pointer-events: none;
  }

  .cover-status.small {
    top: 2px;
    left: 2px;
  }
</style>
