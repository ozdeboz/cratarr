<script lang="ts">
  // An album not in the library that was asked for (the owner, 2026-10-25): on its cover, top left (Play keeps the top
  // right), as Play's round (the owner, 2026-10-25: "the same color style as the play button"; outline only was too
  // faint on covers). Wanted: a clock in the ring; downloading: an
  // arrow in the progress ring, how far. Nothing once it is in the library. What holds it is positioned.
  import type { RequestMark } from './api';
  import Icon from './Icon.svelte';
  import ProgressRing from './ProgressRing.svelte';

  /** `problem`: what went wrong with it (not handed over to Lidarr): the mark a red "!", a tap on it `onproblem` (the
   *  owner, 2026-10-26: not the message in the card's words). */
  let {
    mark,
    small = false,
    problem = null,
    onproblem = null,
  }: { mark: RequestMark | null | undefined; small?: boolean; problem?: string | null; onproblem?: (() => void) | null } = $props();
  const size = $derived(small ? 20 : 30);
  const done = $derived(mark?.state === 'downloading' ? (mark.done ?? 0) : null);
</script>

{#if problem}
  <button
    class="cover-status problem"
    class:small
    aria-label={`Something went wrong: ${problem}`}
    title="Something went wrong"
    style:width="{size}px"
    style:height="{size}px"
    onclick={(e) => {
      e.stopPropagation();
      onproblem?.();
    }}><Icon name="bang" size={small ? 14 : 20} /></button
  >
{:else if mark}
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

  /* What went wrong: the danger's colour, and it takes its own tap. */
  .cover-status.problem {
    padding: 0;
    border: 0;
    background: var(--danger);
    color: #fff;
    pointer-events: auto;
    cursor: pointer;
  }

  .cover-status.small {
    top: 2px;
    left: 2px;
  }
</style>
