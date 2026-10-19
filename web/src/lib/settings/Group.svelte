<script lang="ts">
  // A group of settings (the settings' North Star, the owner 2026-10-19): a small muted heading, an (i) when it has more
  // to say, then its rows (lib/settings/Item) in one rounded block, a line between them. What every row of it shares is
  // said once, here (Players: Speaker delay over each player's row). `tail`: a control at the heading's end (the Log's
  // filter).
  import type { Snippet } from 'svelte';
  import MoreInfo from '../MoreInfo.svelte';

  let {
    title = null,
    more = null,
    tail,
    children,
  }: {
    title?: string | null;
    more?: string | null;
    tail?: Snippet;
    children: Snippet;
  } = $props();
</script>

<div class="group" role="group" aria-label={title ?? undefined}>
  {#if title || tail}
    <h3>
      {#if title}<span>{title}</span>{/if}{#if title && more}<MoreInfo label={title} text={more} />{/if}
      {#if tail}<span class="tail">{@render tail()}</span>{/if}
    </h3>
  {/if}
  <div class="block">{@render children()}</div>
</div>

<style>
  .group + :global(.group) {
    margin-top: 20px;
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 24px;
    margin: 0 4px 6px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }

  .tail {
    display: flex;
    margin-left: auto;
  }

  .block {
    overflow: hidden;
    border-radius: 14px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .block > :global(* + *) {
    border-top: 1px solid var(--border);
  }
</style>
