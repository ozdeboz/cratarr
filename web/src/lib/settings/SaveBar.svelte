<script lang="ts">
  // What is about to be done, at the foot of the screen while it waits: unsaved changes (Save, Discard: the settings'
  // editor's and Kodi's own, lib/KodiSettings) or a review's (decision 0092: "Write 11", its count beside).
  let {
    label = 'Unsaved changes',
    action = 'Save',
    saving = false,
    canSave = true,
    onsave,
    ondiscard = null,
  }: {
    label?: string;
    action?: string;
    saving?: boolean;
    canSave?: boolean;
    onsave: () => void;
    ondiscard?: (() => void) | null;
  } = $props();
</script>

<div class="savebar" role="region" aria-label={label}>
  <span>{label}</span>
  <button class="primary" disabled={saving || !canSave} aria-busy={saving} onclick={onsave}>{action}</button>
  {#if ondiscard}<button disabled={saving} onclick={ondiscard}>Discard</button>{/if}
</div>

<style>
  .savebar {
    position: sticky;
    bottom: calc(8px + env(safe-area-inset-bottom));
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 20px;
    padding: 10px 14px;
    border: 1px solid var(--accent);
    border-radius: 12px;
    background: var(--surface);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }

  @media (min-width: 1024px) {
    .savebar {
      bottom: 16px;
    }
  }

  span {
    margin-right: auto;
    font-weight: 600;
  }

  button {
    min-height: 40px;
    padding: 0 16px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
