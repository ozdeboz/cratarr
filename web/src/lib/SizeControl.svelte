<script lang="ts">
  // Size: smaller and larger for a grid of items, as columns. `value` indexes `widths` (an item's least width). The
  // grid stretches its items to fill a row, so two sizes can give the same columns at a width: a step goes to the next
  // size that changes them, and a button is off when none would.
  import Icon from './Icon.svelte';
  import { nextZoom } from './zoom';

  let { widths, width, value = $bindable() }: { widths: readonly number[]; width: number; value: number } = $props();

  const smaller = $derived(nextZoom(widths, width, value, -1));
  const larger = $derived(nextZoom(widths, width, value, 1));
</script>

<div class="seg" role="group" aria-label="Size">
  <button aria-label="Smaller" disabled={smaller === null} onclick={() => smaller !== null && (value = smaller)}><Icon name="zoomOut" size={22} /></button>
  <button aria-label="Larger" disabled={larger === null} onclick={() => larger !== null && (value = larger)}><Icon name="zoomIn" size={22} /></button>
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

  button {
    display: grid;
    place-items: center;
    min-height: 44px;
    padding: 0 12px;
    background: var(--surface);
    border: 0;
    color: var(--text);
    cursor: pointer;
  }

  button + button {
    border-left: 1px solid var(--border);
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
