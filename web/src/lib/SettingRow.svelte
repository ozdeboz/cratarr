<script lang="ts">
  // A setting's row (decided 2026-10-16: shared pieces): its name (and a muted hint under it) at the start, its control
  // at the end. Where both do not fit, the control goes under the name, as wide as the row: the row wraps, so no rule
  // of a page decides it (a page's narrow-screen rule turned rows into columns once, 2026-10-16). `card`: on a rounded
  // card of its own, as View's rows. `more`: a longer explanation behind an (i) after the name (lib/MoreInfo).
  import type { Snippet } from 'svelte';
  import MoreInfo from './MoreInfo.svelte';

  let {
    label,
    hint = null,
    card = false,
    more = null,
    children,
  }: {
    label: string;
    hint?: string | null;
    card?: boolean;
    more?: string | null;
    /** The control. */
    children: Snippet;
  } = $props();
</script>

<div class="row" class:card role="group" aria-label={label}>
  <span class="name">{label}{#if more}<MoreInfo {label} text={more} />{/if}{#if hint}<small>{hint}</small>{/if}</span>
  <span class="control">{@render children()}</span>
</div>

<style>
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 12px;
    min-width: 0;
    min-height: 52px;
    padding: 6px 0;
  }

  .row.card {
    min-height: 56px;
    padding: 6px 12px 6px 16px;
    border-radius: 12px;
    background: var(--card, color-mix(in srgb, var(--text) 6%, transparent));
  }

  /* The name takes what is left, at least enough to read; the control its own width, never wider than the row. */
  .name {
    flex: 1 1 8em;
    min-width: 0;
    font-size: 15px;
  }

  .name small {
    display: block;
    margin-top: 2px;
    color: var(--muted);
    font-size: 13px;
  }

  .control {
    display: flex;
    flex: 0 1 auto;
    justify-content: flex-end;
    max-width: 100%;
    min-width: 0;
  }
</style>
