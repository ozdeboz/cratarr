<script lang="ts">
  // One way to show a release (an album, a playlist, a crate as an item) in the panels of Requests, Downloads,
  // Insights, Health and Settings (the owner, 2026-10-24: "a unified way of presenting release rows"), in a group of
  // rows (lib/settings/Group, which draws the block and the lines between): its cover, its title, one line under it
  // (`tone`: bad or warn), filled behind as far as work on it has come (`fill`, 0 to 1), and at its end either › (a tap
  // acts: `onclick`) or something of its own (`children`: a tick in a select mode, a count). `open`: the one shown
  // beside. `rank`: its place in a top list, before its cover.
  import type { Snippet } from 'svelte';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';

  let {
    cover,
    title,
    rank = null,
    line = null,
    tone = null,
    fill = null,
    open = false,
    onclick = null,
    children,
  }: {
    cover: string | null;
    title: string;
    rank?: number | null;
    line?: string | null;
    tone?: 'bad' | 'warn' | null;
    fill?: number | null;
    open?: boolean;
    onclick?: (() => void) | null;
    children?: Snippet;
  } = $props();
</script>

{#snippet body()}
  {#if rank !== null}<span class="rank">{rank}</span>{/if}
  <span class="pic"><Cover src={cover} /></span>
  <span class="text">
    <span class="title">{title}</span>
    {#if line}<small class:bad={tone === 'bad'} class:warn={tone === 'warn'}>{line}</small>{/if}
  </span>
{/snippet}

{#if onclick && !children}
  <button
    class="release"
    class:open
    class:filling={fill !== null}
    style:--fill={fill === null ? null : `${Math.min(1, Math.max(0, fill)) * 100}%`}
    aria-pressed={open}
    {onclick}
  >
    {@render body()}
    <span class="end"><Icon name="chevronRight" size={20} /></span>
  </button>
{:else}
  <div class="release still" class:open class:filling={fill !== null} style:--fill={fill === null ? null : `${Math.min(1, Math.max(0, fill)) * 100}%`}>
    {@render body()}
    {#if children}<span class="end">{@render children()}</span>{/if}
  </div>
{/if}

<style>
  .release {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    min-height: 64px;
    padding: 10px 14px 10px 12px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
  }

  button.release {
    cursor: pointer;
  }

  button.release:hover,
  button.release:focus-visible {
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  /* Filled as far as its work has come (a download, a fetch). */
  .release.filling {
    background: linear-gradient(to right, color-mix(in srgb, var(--accent) 14%, transparent) var(--fill), transparent var(--fill));
  }

  /* The one shown beside. */
  .release.open {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .rank {
    flex: none;
    width: 1.4rem;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .pic {
    flex: none;
    width: 44px;
    height: 44px;
    overflow: hidden;
    border-radius: 6px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .pic :global(img) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .title {
    overflow: hidden;
    font-size: 15px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  small {
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  small.bad {
    color: var(--danger-ink);
  }

  small.warn {
    color: var(--warn-ink);
  }

  .end {
    display: grid;
    flex: none;
    place-items: center;
    color: var(--muted);
  }
</style>
