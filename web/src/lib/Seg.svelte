<script lang="ts" generics="T extends string | boolean">
  // One of a few short choices (two or three, a page's layout, a direction): a rounded track, the chosen one in the
  // accent. Words or icons. More choices, or long ones, are a row that opens a sheet (lib/settings/ChoiceSheet).
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
      onclick={() => (value = o.value)}>{#if o.icon}<Icon name={o.icon} size={20} />{:else}{o.label}{/if}</button
    >
  {/each}
</div>

<style>
  /* One rounded track, the choice an accent thumb in it (the owner, 2026-10-25: mockup B, for "our squarish
     multi-pill selector" everywhere): low, as wide as its words, so it sits beside a row's name. */
  .seg {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    max-width: 100%;
    padding: 3px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 9%, transparent);
  }

  /* A choice's words on one line: where they do not fit, the row puts the whole control under its name instead. */
  button {
    min-height: 30px;
    padding: 0 14px;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 0.93em;
    white-space: nowrap;
    cursor: pointer;
  }

  button.icon {
    display: grid;
    place-items: center;
    padding: 0 10px;
  }

  button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }
</style>
