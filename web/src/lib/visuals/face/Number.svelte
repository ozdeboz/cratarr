<script lang="ts">
  // A Number (face.ts): a value as a clear figure, its name beside it where given.
  import { numberText } from './face';

  let { value, label }: { value: number | null; label?: string } = $props();
</script>

<div class="number">
  {#if label}<span class="label">{label}</span>{/if}
  <span class="value">{numberText(value)}</span>
</div>

<style>
  .number {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    height: 18px;
    font-size: 12px;
  }
  .label {
    color: var(--muted);
  }
  .value {
    color: var(--text);
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
</style>
