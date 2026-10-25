<script lang="ts">
  // Play on a Discover suggestion's cover (decision 0071, the owner 2026-10-25: every item that plays on Bandcamp gets
  // one, "a tad larger", and it plays at once): the accent's round button in the cover's corner, beside the card's own
  // button (a button cannot hold another), over it. What holds it is positioned.
  import Icon from './Icon.svelte';

  /** `problem`: why it last failed to play: a red "!" in its place, a tap on it `onproblem` (the owner, 2026-10-26). */
  let {
    label,
    busy = false,
    problem = null,
    onplay,
    onproblem = null,
  }: { label: string; busy?: boolean; problem?: string | null; onplay: () => void; onproblem?: (() => void) | null } = $props();
</script>

<button
  class="cover-play"
  class:problem={!!problem}
  aria-label={problem ? `${label} did not play: ${problem}` : `Play ${label} on Bandcamp`}
  title={problem ? 'It did not play' : 'Play on Bandcamp'}
  disabled={busy}
  aria-busy={busy}
  onclick={(e) => {
    e.stopPropagation();
    if (problem && onproblem) onproblem();
    else onplay();
  }}><Icon name={problem ? 'bang' : 'play'} size={problem ? 20 : 17} /></button
>

<style>
  .cover-play.problem {
    background: var(--danger) !important;
    color: #fff !important;
  }

  .cover-play {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.35);
    cursor: pointer;
  }

  .cover-play:disabled {
    opacity: 0.7;
    cursor: progress;
  }
</style>
