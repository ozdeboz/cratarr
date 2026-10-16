<script lang="ts">
  // A card (decided 2026-10-16: shared pieces, not each sheet's own): a rounded surface for a part of a sheet or a
  // setting, its title (and a muted note after it) above what it holds. The sheets' cards and Settings' are this.
  import type { Snippet } from 'svelte';

  let { title = null, note = null, label = null, children }: { title?: string | null; note?: string | null; label?: string | null; children: Snippet } = $props();
</script>

<div class="card" role={label ? 'group' : undefined} aria-label={label ?? undefined}>
  {#if title}<div class="head"><span class="title">{title}</span>{#if note}<small>{note}</small>{/if}</div>{/if}
  {@render children()}
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 6px 12px 6px 16px;
    border-radius: 12px;
    background: var(--card, color-mix(in srgb, var(--text) 6%, transparent));
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 6px;
    padding: 10px 0 4px;
  }

  .title {
    font-size: 15px;
    font-weight: 600;
  }

  small {
    color: var(--muted);
    font-size: 13px;
  }
</style>
