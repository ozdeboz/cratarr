<script lang="ts">
  // The tempo spread's scale (decision 0041), under the spread or under a selector's handles.
  import { HI, LO } from './Spread.svelte';
</script>

<div class="axis" aria-hidden="true"><span>{LO}</span><span>{(LO + HI) / 2}</span><span>{HI} BPM</span></div>

<style>
  .axis {
    display: flex;
    justify-content: space-between;
    font-size: 0.72rem;
    color: var(--muted);
  }
</style>
