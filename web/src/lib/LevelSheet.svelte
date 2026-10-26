<script lang="ts">
  // A long press on the player's Level button (the owner, 2026-10-26): its controls at hand, the same rows as This
  // device's Playing (lib/settings/LevelItems).
  import MenuSheet from './MenuSheet.svelte';
  import Group from './settings/Group.svelte';
  import LevelItems from './settings/LevelItems.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet label="Level the volume" title="Level the volume" {onclose}>
  <div class="rows">
    <Group><LevelItems /></Group>
  </div>
</MenuSheet>

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
