<script lang="ts">
  // Play on a Discover suggestion's cover (decision 0071, the owner 2026-10-25: every item that plays on Bandcamp gets
  // one, "a tad larger", and it plays at once): the accent's round button in the cover's corner, beside the card's own
  // button (a button cannot hold another), over it. What holds it is positioned.
  import Icon from './Icon.svelte';

  let { label, busy = false, onplay }: { label: string; busy?: boolean; onplay: () => void } = $props();
</script>

<button
  class="cover-play"
  aria-label={`Play ${label} on Bandcamp`}
  title="Play on Bandcamp"
  disabled={busy}
  aria-busy={busy}
  onclick={(e) => {
    e.stopPropagation();
    onplay();
  }}><Icon name="play" size={17} /></button
>

<style>
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
