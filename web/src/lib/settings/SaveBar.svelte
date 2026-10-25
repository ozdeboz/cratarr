<script lang="ts">
  // Unsaved changes, at the foot of the screen while there are any: Save and Discard. The settings' editor's (the
  // panel's) and Kodi's own (lib/KodiSettings, saved apart).
  let {
    saving = false,
    canSave = true,
    onsave,
    ondiscard,
  }: {
    saving?: boolean;
    canSave?: boolean;
    onsave: () => void;
    ondiscard: () => void;
  } = $props();
</script>

<div class="savebar" role="region" aria-label="Unsaved changes">
  <span>Unsaved changes</span>
  <button class="primary" disabled={saving || !canSave} aria-busy={saving} onclick={onsave}>Save</button>
  <button disabled={saving} onclick={ondiscard}>Discard</button>
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
