<script lang="ts">
  // A tap on the player's Level button (the owner, 2026-10-26; a tap since 2026-10-27): its controls at hand, the same
  // rows as This device's Playing (lib/settings/LevelItems), for the player that plays: this device, the one driven, or
  // a group as one, named in its chip (the owner, 2026-10-27: explicit what it controls).
  import { levelTarget } from './level.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Group from './settings/Group.svelte';
  import LevelItems from './settings/LevelItems.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet label="Level the volume" title="Level the volume" chips={[levelTarget().label]} {onclose}>
  <div class="rows">
    <Group><LevelItems playing /></Group>
  </div>
</MenuSheet>

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
