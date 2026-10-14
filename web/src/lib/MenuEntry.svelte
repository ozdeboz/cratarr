<script lang="ts">
  // One entry in a menu sheet (decision 0021), as Finamp's: a 56 px row, a 24 px icon in the accent colour, the label
  // at 16 px. Destructive entries stay Cratarr's way: red (and last in the list). `checked` makes it one of Finamp's
  // toggle cards (the playlists for a track): filled with the accent, stronger when on, a mark at the right.
  import Icon, { type IconName } from './Icon.svelte';

  let {
    icon = null,
    label,
    onclick,
    disabled = false,
    danger = false,
    checked = null,
  }: {
    icon?: IconName | null;
    label: string;
    onclick: () => void;
    disabled?: boolean;
    danger?: boolean;
    checked?: boolean | null;
  } = $props();
</script>

{#if checked === null}
  <button class="entry" class:danger role="menuitem" {disabled} {onclick}>
    {#if icon}<Icon name={icon} size={24} />{/if}<span>{label}</span>
  </button>
{:else}
  <button class="toggle" class:on={checked} role="menuitemcheckbox" aria-checked={checked} {disabled} {onclick}>
    <span>{label}</span><i aria-hidden="true"></i><Icon name={checked ? 'check' : 'plus'} size={22} />
  </button>
{/if}

<style>
  .entry {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 56px;
    padding: 0 24px 0 24px;
    background: none;
    border: 0;
    color: var(--text);
    font: inherit;
    font-size: 16px;
    text-align: left;
    cursor: pointer;
  }

  .entry :global(svg) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  .entry:hover,
  .entry:focus-visible {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  .entry:disabled {
    color: color-mix(in srgb, var(--text) 38%, transparent);
    cursor: default;
  }

  .entry:disabled :global(svg) {
    opacity: 0.5;
  }

  .entry.danger,
  .entry.danger :global(svg) {
    color: var(--danger);
  }

  .toggle {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 12px;
    width: calc(100% - 24px);
    min-height: 48px;
    margin: 4px 12px;
    padding: 0 14px;
    border: 0;
    border-radius: 12px;
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 10%, transparent);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .toggle.on {
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 30%, transparent);
  }

  .toggle span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toggle i {
    align-self: stretch;
    width: 1.5px;
    margin: 10px 0;
    background: color-mix(in srgb, var(--text) 50%, transparent);
  }

  .toggle :global(svg) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  .toggle:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
