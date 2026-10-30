<script lang="ts">
  // A long press on the player's visuals button (the owner, 2026-10-30: only the controls that concern it): the visuals
  // by themselves, the same rows as Settings → This device's (lib/settings/VisualsAutoGroup), as the info button's
  // Details sheet (lib/DetailsSheet).
  import MenuSheet from './MenuSheet.svelte';
  import VisualsAutoGroup from './settings/VisualsAutoGroup.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet label="Visuals" title="Visuals" {onclose}>
  <div class="rows">
    <VisualsAutoGroup heading={false} />
  </div>
</MenuSheet>

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
