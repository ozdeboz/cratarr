<script lang="ts">
  // A value of the settings, typed in its own sheet (the settings' North Star, the owner 2026-10-19): a row says the
  // value, a tap opens this with the field, Done takes it. From the top, as a sheet to type in is (the keyboard would
  // cover one from the bottom). What Done does is the row's: into the editor (saved with the Save bar), or at once.
  // `help`: a line under the field (how to fill it in); `extra`: what the field shows as it is typed (Names' matches);
  // `lines`: several lines (a list, one per line); `suffix`: after a number (days).
  import type { Snippet } from 'svelte';
  import MenuSheet from '../MenuSheet.svelte';

  let {
    title,
    value,
    type = 'text',
    placeholder = '',
    help = null,
    suffix = null,
    min = null,
    max = null,
    lines = false,
    allowEmpty = true,
    saveLabel = 'Done',
    extra,
    oninput = null,
    onsave,
    onclose,
  }: {
    title: string;
    value: string;
    type?: 'text' | 'url' | 'password' | 'number';
    placeholder?: string;
    help?: string | null;
    suffix?: string | null;
    min?: number | null;
    max?: number | null;
    lines?: boolean;
    allowEmpty?: boolean;
    saveLabel?: string;
    extra?: Snippet;
    oninput?: ((v: string) => void) | null;
    onsave: (v: string) => void | Promise<unknown>;
    onclose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let typed = $state(value);
  let busy = $state(false);
  const ok = $derived(allowEmpty || typed.trim() !== '');

  async function done() {
    if (!ok) return;
    busy = true;
    try {
      await onsave(type === 'password' ? typed : typed.trim());
      onclose();
    } catch {
      // Not taken: the sheet stays, and the row's own message says why.
    } finally {
      busy = false;
    }
  }
</script>

<MenuSheet label={title} {title} {onclose}>
  <form class="form" onsubmit={(e) => (e.preventDefault(), done())}>
    <div class="field">
      {#if lines}
        <!-- svelte-ignore a11y_autofocus -->
        <textarea rows="5" aria-label={title} {placeholder} spellcheck="false" autofocus bind:value={typed} oninput={() => oninput?.(typed)}></textarea>
      {:else}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          {type}
          aria-label={title}
          {placeholder}
          min={min ?? undefined}
          max={max ?? undefined}
          spellcheck="false"
          autocapitalize="off"
          autocomplete={type === 'password' ? 'new-password' : 'off'}
          autofocus
          bind:value={typed}
          oninput={() => oninput?.(typed)}
        />
      {/if}
      {#if suffix}<span class="suffix">{suffix}</span>{/if}
    </div>
    {#if help}<p class="help">{help}</p>{/if}
    {@render extra?.()}
    <div class="actions">
      <button type="button" disabled={busy} onclick={onclose}>Cancel</button>
      <button type="submit" class="primary" disabled={busy || !ok} aria-busy={busy}>{saveLabel}</button>
    </div>
  </form>
</MenuSheet>

<style>
  .form {
    display: grid;
    gap: 10px;
    padding: 0 16px 8px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  input,
  textarea {
    flex: 1;
    min-width: 0;
    min-height: 44px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    color: inherit;
    font: inherit;
  }

  textarea {
    padding: 10px 12px;
    resize: vertical;
  }

  input:focus,
  textarea:focus {
    border-color: var(--accent);
    outline: none;
  }

  .suffix,
  .help {
    color: var(--muted);
  }

  .help {
    margin: 0;
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
