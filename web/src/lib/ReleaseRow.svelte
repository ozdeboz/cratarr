<script lang="ts">
  // One way to show a release (an album, a playlist, a crate as an item) in the panels of Requests, Downloads,
  // Insights, Health and Settings (the owner, 2026-10-24: "a unified way of presenting release rows"), in a group of
  // rows (lib/settings/Group, which draws the block and the lines between): its cover, its title, one line under it
  // (`tone`: bad or warn), filled behind as far as work on it has come (`fill`, 0 to 1), and at its end either › (a tap
  // acts: `onclick`) or something of its own (`children`: a tick in a select mode, a count); both, the row acts and its
  // end is its own (0.13, Health's review list: a tap picks another copy, its tick at the end). `open`: the one shown
  // beside. `rank`: its place in a top list, before its cover. `expanded` (0.13, Health's panel): it opens what it holds
  // under itself, in place, its › turned down while it does.
  import type { Snippet } from 'svelte';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';

  let {
    cover,
    title,
    rank = null,
    line = null,
    tone = null,
    wrap = false,
    fill = null,
    open = false,
    expanded = null,
    onclick = null,
    children,
  }: {
    cover: string | null;
    title: string;
    rank?: number | null;
    line?: string | null;
    tone?: 'bad' | 'warn' | null;
    /** Its line in full, over as many lines as it needs (a reason, some long), not cut at the row's end. */
    wrap?: boolean;
    fill?: number | null;
    open?: boolean;
    expanded?: boolean | null;
    onclick?: (() => void) | null;
    children?: Snippet;
  } = $props();
</script>

{#snippet body()}
  {#if rank !== null}<span class="rank">{rank}</span>{/if}
  <span class="pic"><Cover src={cover} /></span>
  <span class="text">
    <span class="title">{title}</span>
    {#if line}<small class:bad={tone === 'bad'} class:warn={tone === 'warn'} class:wrap>{line}</small>{/if}
  </span>
{/snippet}

{#if onclick && !children}
  <button
    class="release"
    class:open
    class:filling={fill !== null}
    style:--fill={fill === null ? null : `${Math.min(1, Math.max(0, fill)) * 100}%`}
    aria-pressed={expanded === null ? open : undefined}
    aria-expanded={expanded ?? undefined}
    {onclick}
  >
    {@render body()}
    <span class="end"><Icon name={expanded ? 'chevronDown' : 'chevronRight'} size={20} /></span>
  </button>
{:else if onclick}
  <!-- It acts, and its end is a control of its own (two controls side by side, never one inside the other). -->
  <div class="release both" class:open class:filling={fill !== null} style:--fill={fill === null ? null : `${Math.min(1, Math.max(0, fill)) * 100}%`}>
    <button class="main" {onclick}>{@render body()}</button>
    <span class="end">{@render children?.()}</span>
  </div>
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

  /* It acts and has an end of its own: its body the button, the end beside it. */
  .release.both {
    padding: 0 14px 0 0;
  }

  .release.both .main {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 14px;
    min-width: 0;
    min-height: 64px;
    padding: 10px 0 10px 12px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .release.both .main:hover,
  .release.both .main:focus-visible {
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

  small.wrap {
    white-space: normal;
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

  /* Its own buttons as a settings row's (lib/settings/Item): small and flat, the main one in the accent; a tick is its
     own. */
  .end :global(button:where(:not(.tick, [role='checkbox'], [aria-pressed]))) {
    min-height: 36px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .end :global(button.primary) {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .end :global(button:disabled) {
    opacity: 0.5;
    cursor: default;
  }
</style>
