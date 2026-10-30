<script lang="ts">
  // Symfonium's select bar (decided 2026-10-15), wherever a list has a select mode: the panel's track lists and the
  // Playlists page (2026-10-16). It floats over the list's end while selecting: how many of how many, ✕ (Select ends),
  // and what to do with the ticked ones (`children`: icon buttons; `fav` fills a heart). In the panel it stands over
  // the panel's end; on a page it stays at the bottom of the window as the list scrolls.
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    label,
    count,
    onclose,
    place = 'panel',
    every,
    children,
  }: {
    /** What is ticked, for screen readers ("Selected tracks"). */
    label: string;
    /** How many of how many ("3/12"). */
    count: string;
    onclose: () => void;
    place?: 'panel' | 'page';
    /** All or none at once (the owner, 2026-10-25): the top row's left button, "All", or "None" with every one ticked. */
    every?: { all: boolean; onall: () => void; onnone: () => void } | null;
    children: Snippet;
  } = $props();
</script>

<div class="selbar {place}" role="toolbar" aria-label={label}>
  <div class="selhead">
    {#if every}
      <button class="every" onclick={() => (every.all ? every.onnone() : every.onall())}>{every.all ? 'None' : 'All'}</button>
    {:else}
      <span></span>
    {/if}
    <span class="selcount">{count}</span>
    <button class="close" aria-label="Stop selecting" title="Stop selecting" onclick={onclose}><Icon name="close" size={22} /></button>
  </div>
  <div class="selacts">{@render children()}</div>
</div>

<style>
  .selbar {
    z-index: 4;
    width: min(300px, calc(100% - 32px));
    border-radius: 16px;
    /* The floating controls' surface (app.css): see-through, blurring what is under it. */
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.35);
  }

  /* Over the panel's end, above the mini-player floating there. */
  .selbar.panel {
    position: absolute;
    right: 16px;
    bottom: calc(16px + var(--mini-h, 0px) + env(safe-area-inset-bottom));
  }

  /* At the end of the page's list, staying at the window's bottom (above the mini-player) while it scrolls. */
  .selbar.page {
    position: sticky;
    bottom: calc(16px + var(--bottom-h, 0px) + env(safe-area-inset-bottom));
    margin: 16px 0 0 auto;
    width: min(300px, 100%);
  }

  .selhead {
    display: grid;
    grid-template-columns: 48px 1fr 48px;
    align-items: center;
    padding: 0 4px;
    border-bottom: 1px solid color-mix(in srgb, var(--text) 15%, transparent);
  }

  .every {
    min-height: 40px;
    padding: 0 4px;
    border: 0;
    border-radius: 20px;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
  }

  .selcount {
    text-align: center;
    font-size: 15px;
  }

  .selacts {
    display: flex;
    justify-content: space-evenly;
    padding: 4px;
  }

  .close,
  .selacts :global(button) {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .selacts :global(button:disabled) {
    opacity: 0.4;
    cursor: default;
  }

  .selacts :global(button.fav svg) {
    fill: currentColor;
  }
</style>
