<script lang="ts">
  // The one on/off control (Material 3's switch, as Symfonium's): a row with its label, an optional hint under it, and
  // the switch at the end. `card`: on a card of its own, as in the sheets (several in a row join into one rounded
  // group); otherwise a plain row, as in Settings. `hintWhenOn`: the hint only while on, sliding open. `hideLabel`: the
  // switch alone (the label then names it for screen readers). `more`: a longer explanation behind an (i) beside the
  // switch (lib/MoreInfo), a button of its own next to the switch's, never inside it.
  import { slide } from 'svelte/transition';
  import MoreInfo from './MoreInfo.svelte';
  import SwitchTrack from './SwitchTrack.svelte';

  let {
    checked,
    onchange,
    label,
    hint = null,
    hintWhenOn = false,
    hideLabel = false,
    card = false,
    disabled = false,
    more = null,
  }: {
    checked: boolean;
    onchange: (on: boolean) => void;
    label: string;
    hint?: string | null;
    hintWhenOn?: boolean;
    hideLabel?: boolean;
    card?: boolean;
    disabled?: boolean;
    more?: string | null;
  } = $props();
</script>

{#snippet theSwitch()}
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
  {#if more && !hideLabel}<span class="room" aria-hidden="true"></span>{/if}
  <SwitchTrack on={checked} />
</button>
{/snippet}

{#if more && !hideLabel}
  <div class="with-more" class:card>
    {@render theSwitch()}
    <span class="more"><MoreInfo {label} text={more} /></span>
  </div>
{:else}
  {@render theSwitch()}
{/if}

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

  /* The (i)'s place in the row, between the label and the track; the (i) itself lies over it, outside the switch. */
  .room {
    flex: none;
    width: 32px;
  }

  .with-more {
    position: relative;
    flex: 1 1 100%;
    min-width: 0;
  }

  .more {
    position: absolute;
    top: 50%;
    right: 68px;
    display: flex;
    transform: translateY(-50%);
  }

  .with-more.card .more {
    right: 84px;
  }

  .switch:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
