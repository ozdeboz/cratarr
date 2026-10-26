<script lang="ts">
  // A long press on a play button (the owner, 2026-10-26: its controls at hand, not the button's word): Start and stop,
  // the same rows as This device's Playing (lib/settings/StartStopItems).
  import MenuSheet from './MenuSheet.svelte';
  import Group from './settings/Group.svelte';
  import StartStopItems from './settings/StartStopItems.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet label="Start and stop" title="Start and stop" {onclose}>
  <div class="rows">
    <Group><StartStopItems /></Group>
  </div>
</MenuSheet>

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
