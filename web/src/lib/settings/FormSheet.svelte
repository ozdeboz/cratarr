<script lang="ts">
  // A few fields that go together, typed in one sheet (a new password with the current one, a new key): as a value's
  // sheet (lib/settings/ValueSheet), from the top, its act and Cancel at its foot. The fields are its children: inputs,
  // selects and a choice of a few (lib/Seg), each named by a label.
  import type { Snippet } from 'svelte';
  import MenuSheet from '../MenuSheet.svelte';

  let {
    title,
    submitLabel,
    canSubmit = true,
    busy = false,
    error = null,
    onsubmit,
    onclose,
    children,
  }: {
    title: string;
    submitLabel: string;
    canSubmit?: boolean;
    busy?: boolean;
    error?: string | null;
    onsubmit: () => void;
    onclose: () => void;
    children: Snippet;
  } = $props();
</script>

<MenuSheet label={title} {title} {onclose}>
  <form class="form" onsubmit={(e) => (e.preventDefault(), canSubmit && !busy && onsubmit())}>
    {@render children()}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <button type="button" disabled={busy} onclick={onclose}>Cancel</button>
      <button type="submit" class="primary" disabled={busy || !canSubmit} aria-busy={busy}>{submitLabel}</button>
    </div>
  </form>
</MenuSheet>

<style>
  .form {
    display: grid;
    gap: 10px;
    padding: 0 16px 8px;
  }

  .form :global(label) {
    display: grid;
    gap: 4px;
    color: var(--muted);
    font-size: 13px;
  }

  .form :global(input),
  .form :global(select) {
    min-width: 0;
    min-height: 44px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  .form :global(input:focus),
  .form :global(select:focus) {
    border-color: var(--accent);
    outline: none;
  }

  .error {
    margin: 0;
    color: var(--danger-ink);
    font-size: 13px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .actions button {
    min-height: 40px;
    padding: 0 16px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  .actions .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
