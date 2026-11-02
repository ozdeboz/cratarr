<script lang="ts">
  // A section's Reset (the owner, 2026-10-24: every settings section with an order, a reset at the right of its heading;
  // lib/settings/Group's `tail`): its part's declaration (lib/settings/resetItems), its section's only, asked in the same
  // sheet as the part's Reset in the top right glass (the owner, 2026-11-02, lib/settings/ResetSheet). Dimmed while
  // nothing of it differs.
  import Icon from '../Icon.svelte';
  import { resetOf, rowsOf, type ResetItem } from './reset.svelte';
  import ResetSheet from './ResetSheet.svelte';

  let { items, section, label = section }: { items: () => ResetItem[]; section: string; label?: string } = $props();

  const rows = $derived(rowsOf(items(), section));
  let asking = $state(false);
</script>

<button class="reset" disabled={!rows.length} aria-label={`Reset ${label.toLowerCase()}`} title="Reset" onclick={() => (asking = true)}><Icon name="reset" size={18} /></button>
{#if asking && rows.length}
  <ResetSheet title={`Reset ${label.toLowerCase()}`} {rows} onconfirm={() => resetOf(items(), section)} onclose={() => (asking = false)} />
{/if}

<style>
  .reset {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    margin: -6px 0;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .reset:hover:not(:disabled),
  .reset:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
  }

  .reset:disabled {
    opacity: 0.35;
    cursor: default;
  }
</style>
