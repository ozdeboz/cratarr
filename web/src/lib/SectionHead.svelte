<script lang="ts">
  // A section's heading in a panel's page (as Symfonium's: More from, About, Albums, Top tracks): large and plain, with
  // a › at the end when the heading opens all of it (`onmore`, the whole heading is the button then); or a play button
  // at its end (`onplay`: Home's sections that play as a whole, the owner 2026-10-23).
  import Icon from './Icon.svelte';

  /** `onrefresh`: a refresh at its end (Discover's Around artists: other artists, the owner 2026-10-26). `onback`: back
   *  before it, out of what it shows whole (the search's Show all: the owner, 2026-10-30, there was no way back). */
  let {
    title,
    onmore,
    onplay,
    onrefresh,
    onback,
    busy = false,
  }: { title: string; onmore?: () => void; onplay?: () => void; onrefresh?: () => void; onback?: () => void; busy?: boolean } = $props();
</script>

{#if onback}
  <div class="head">
    <button class="back" aria-label="Back" title="Back" onclick={onback}><Icon name="back" size={22} /></button>
    <h2>{title}</h2>
  </div>
{:else if onplay}
  <div class="head">
    <h2>{title}</h2>
    <button class="play" aria-label={`Play ${title}`} title="Play" disabled={busy} onclick={onplay}><Icon name="play" size={20} /></button>
  </div>
{:else if onrefresh}
  <div class="head">
    <h2>{title}</h2>
    <button class="play" aria-label={`Refresh ${title}`} title="Refresh" disabled={busy} onclick={onrefresh}><Icon name="refresh" size={20} /></button>
  </div>
{:else if onmore}
  <button class="head" onclick={onmore}>
    <h2>{title}</h2>
    <span class="more"><Icon name="chevronRight" /></span>
  </button>
{:else}
  <div class="head"><h2>{title}</h2></div>
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    box-sizing: border-box;
    padding: 8px 12px 8px 16px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
  }

  button.head {
    cursor: pointer;
  }

  h2 {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 23px;
    font-weight: 400;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .more {
    display: flex;
    color: var(--accent-ink);
  }

  /* Back, before the heading: a bare icon in the text's ink, the heading after it. */
  .back {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
  }

  .back + h2 {
    flex: 1;
  }

  .play {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    cursor: pointer;
  }

  .play:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
