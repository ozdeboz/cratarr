<script lang="ts">
  // Size: smaller and larger for a grid of items, as columns. `value` indexes `widths` (an item's least width). The
  // grid stretches its items to fill a row, so two sizes can give the same columns at a width: a step goes to the next
  // size that changes them, and a button is off when none would. `plain`: two bare round icons, no frame (in a menu
  // sheet beside its heading, as its back arrow is); framed in the View sheet's rows.
  import Icon from './Icon.svelte';
  import { nextZoom } from './zoom';

  let { widths, width, value = $bindable(), plain = false }: { widths: readonly number[]; width: number; value: number; plain?: boolean } = $props();

  const smaller = $derived(nextZoom(widths, width, value, -1));
  const larger = $derived(nextZoom(widths, width, value, 1));
</script>

<div class="seg" class:plain role="group" aria-label="Size">
  <button aria-label="Smaller" disabled={smaller === null} onclick={() => smaller !== null && (value = smaller)}><Icon name="zoomOut" size={plain ? 22 : 20} /></button>
  <button aria-label="Larger" disabled={larger === null} onclick={() => larger !== null && (value = larger)}><Icon name="zoomIn" size={plain ? 22 : 20} /></button>
</div>

<style>
  /* As the choices' track (lib/Seg, the owner 2026-10-25): one rounded track, its two buttons round in it, low; actions,
     not a choice, so neither is in the accent. */
  .seg {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 9%, transparent);
  }

  button {
    display: grid;
    place-items: center;
    min-width: 40px;
    min-height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--text);
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  @media (hover: hover) {
    .seg:not(.plain) button:not(:disabled):hover {
      background: color-mix(in srgb, var(--text) 10%, transparent);
    }
  }

  .plain {
    gap: 4px;
    padding: 0;
    background: none;
  }

  .plain button {
    width: 40px;
    min-height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
  }

  /* Only with a pointer that hovers: a finger's tap would leave the tint on. */
  @media (hover: hover) {
    .plain button:not(:disabled):hover {
      background: color-mix(in srgb, var(--text) 10%, transparent);
    }
  }
</style>
