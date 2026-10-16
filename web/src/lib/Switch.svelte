<script lang="ts">
  // The one on/off control (Material 3's switch, as Symfonium's): a row with its label, an optional hint under it, and
  // the switch at the end. `card`: on a card of its own, as in the sheets (several in a row join into one rounded
  // group); otherwise a plain row, as in Settings. `hintWhenOn`: the hint only while on, sliding open. `hideLabel`: the
  // switch alone (the label then names it for screen readers).
  import { slide } from 'svelte/transition';

  let {
    checked,
    onchange,
    label,
    hint = null,
    hintWhenOn = false,
    hideLabel = false,
    card = false,
    disabled = false,
  }: {
    checked: boolean;
    onchange: (on: boolean) => void;
    label: string;
    hint?: string | null;
    hintWhenOn?: boolean;
    hideLabel?: boolean;
    card?: boolean;
    disabled?: boolean;
  } = $props();
</script>

<button
  class="switch"
  class:card
  class:bare={hideLabel}
  role="switch"
  aria-checked={checked}
  aria-label={hideLabel ? label : undefined}
  {disabled}
  onclick={() => onchange(!checked)}
>
  {#if !hideLabel}
    <span class="text">
      <span class="label">{label}</span>
      {#if hint && (!hintWhenOn || checked)}<small transition:slide={{ duration: hintWhenOn ? 150 : 0 }}>{hint}</small>{/if}
    </span>
  {/if}
  <i aria-hidden="true"></i>
</button>

<style>
  .switch {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 48px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .switch.bare {
    width: auto;
    min-height: 0;
  }

  .switch.card {
    min-height: 64px;
    padding: 8px 16px 8px 20px;
    border-radius: 6px;
    background: var(--card);
    font-size: 16px;
  }

  .switch.card:first-child {
    border-top-left-radius: 20px;
    border-top-right-radius: 20px;
  }

  .switch.card:last-child {
    border-bottom-left-radius: 20px;
    border-bottom-right-radius: 20px;
  }

  .switch.card:hover:not(:disabled) {
    background: var(--card-hover);
  }

  .switch:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .text {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  small {
    color: var(--muted);
    font-size: 0.8rem;
  }

  /* A 52 × 32 track, its thumb growing and moving over when on, the track then filled with the accent. */
  i {
    position: relative;
    flex: none;
    box-sizing: border-box;
    width: 52px;
    height: 32px;
    border: 2px solid var(--muted);
    border-radius: 16px;
    transition:
      background-color 150ms,
      border-color 150ms;
  }

  i::after {
    content: '';
    position: absolute;
    top: 50%;
    left: 6px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--muted);
    transform: translateY(-50%);
    transition:
      left 150ms,
      width 150ms,
      height 150ms,
      background-color 150ms;
  }

  .switch[aria-checked='true'] i {
    border-color: var(--art-accent, var(--accent));
    background: var(--art-accent, var(--accent));
  }

  .switch[aria-checked='true'] i::after {
    left: 22px;
    width: 24px;
    height: 24px;
    background: var(--accent-text);
  }

  .switch:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
