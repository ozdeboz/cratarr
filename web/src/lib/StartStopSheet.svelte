<script lang="ts">
  // A long press on a play button (the owner, 2026-10-26: its controls at hand, not the button's word): the tape, its
  // groups as This device's Tape page has them (lib/settings/TapeItems).
  import MenuSheet from './MenuSheet.svelte';
  import TapeItems from './settings/TapeItems.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet label="Tape" title="Tape" {onclose}>
  <div class="rows">
    <TapeItems />
  </div>
</MenuSheet>

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
