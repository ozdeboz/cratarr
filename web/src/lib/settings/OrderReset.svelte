<script lang="ts">
  // A group's order back to its default (the owner, 2026-10-24: every settings section with an order, a reset at the
  // right of its heading; lib/settings/Group's `tail`). Dimmed while it is the default already; Undo in the toast.
  import Icon from '../Icon.svelte';
  import { toast } from '../toast.svelte';

  let { label, isDefault, reset }: { label: string; isDefault: boolean; reset: () => () => void } = $props();

  function run() {
    const undo = reset();
    toast.show({ text: `${label} reset.`, tone: 'done', act: { label: 'Undo', run: undo } });
  }
</script>

<button class="reset" disabled={isDefault} aria-label={`Reset ${label.toLowerCase()}`} title="Reset" onclick={run}><Icon name="reset" size={18} /></button>

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
