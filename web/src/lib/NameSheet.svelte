<script lang="ts">
  // The sheet that asks for a new thing's name (a playlist, a crate): a sheet as every menu (decision 0021), one field
  // and the button that makes it. The same wherever something new is named.
  import MenuSheet from './MenuSheet.svelte';

  let {
    title,
    what,
    action,
    onsubmit,
    onclose,
  }: {
    /** "New playlist". */
    title: string;
    /** What is named, for the field's label: "playlist". */
    what: string;
    /** The button: "Create playlist". */
    action: string;
    /** Makes it; the sheet waits while it does. */
    onsubmit: (name: string) => unknown;
    onclose: () => void;
  } = $props();

  let name = $state('');
  let working = $state(false);
  async function submit() {
    const n = name.trim();
    if (!n || working) return;
    working = true;
    try {
      await onsubmit(n);
    } finally {
      working = false;
    }
  }
</script>

<MenuSheet label={title} {title} {onclose}>
  <form
    class="form"
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
  >
    <input type="text" placeholder="Name" aria-label={`Name of the new ${what}`} maxlength="100" bind:value={name} />
    <button type="submit" class="primary" disabled={working || !name.trim()} aria-busy={working}>{action}</button>
  </form>
</MenuSheet>

<style>
  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .form input {
    min-height: 44px;
    padding: 0 12px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .form button.primary {
    min-height: 44px;
    border: 0;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .form button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
