<script lang="ts">
  // A long press on the player's info button (the owner, 2026-10-26: only the controls that concern it): which facts
  // show beside it, the same rows as Settings → Player's Details (lib/settings/DetailsGroup).
  import MenuSheet from './MenuSheet.svelte';
  import DetailsGroup from './settings/DetailsGroup.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet label="Extra details" title="Extra details" {onclose}>
  <div class="rows">
    <DetailsGroup toggle={false} />
  </div>
</MenuSheet>

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
