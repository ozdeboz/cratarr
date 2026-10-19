<script lang="ts" generics="T extends string | boolean">
  // A segmented switch: one of a few choices, pressed in the accent (a page's order, its layout). Words or icons.
  import Icon, { type IconName } from './Icon.svelte';

  let {
    label,
    options,
    value = $bindable(),
  }: {
    /** The group's accessible name. */
    label: string;
    options: { value: T; label: string; icon?: IconName; title?: string }[];
    value: T;
  } = $props();
</script>

<div class="seg" role="group" aria-label={label}>
  {#each options as o (o.value)}
    <!-- type="button": in a form (a sheet's fields) a choice is no submit. -->
    <button
      type="button"
      class:icon={!!o.icon}
      aria-pressed={value === o.value}
      aria-label={o.icon ? o.label : undefined}
      title={o.title ?? (o.icon ? o.label : undefined)}
      onclick={() => (value = o.value)}>{#if o.icon}<Icon name={o.icon} size={22} />{:else}{o.label}{/if}</button
    >
  {/each}
</div>

<style>
  .seg {
    display: flex;
    align-items: center;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    background: var(--surface);
  }

  /* A choice's words on one line: where they do not fit, the row puts the whole control under its name instead. */
  button {
    min-height: 44px;
    padding: 0 16px;
    background: var(--surface);
    border: 0;
    color: var(--text);
    font: inherit;
    white-space: nowrap;
    cursor: pointer;
  }

  button.icon {
    display: grid;
    place-items: center;
    padding: 0 12px;
  }

  button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }
</style>
