<script lang="ts">
  // A long press on the player's info button (the owner, 2026-10-26: only the controls that concern it): which facts
  // show beside it, the same rows as Settings → Player's Details (lib/settings/DetailsGroup), the reset in the header
  // (the owner, 2026-10-26).
  import MenuSheet from './MenuSheet.svelte';
  import { prefs } from './prefs.svelte';
  import DetailsGroup, { resetFacts } from './settings/DetailsGroup.svelte';
  import { resetWithUndo } from './settings/OrderReset.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet
  label="Extra details"
  title="Extra details"
  end={{ icon: 'reset', label: 'Reset extra details', disabled: !prefs.playerFacts.length, onclick: () => resetWithUndo('Extra details', resetFacts) }}
  {onclose}
>
  <div class="rows">
    <DetailsGroup toggle={false} />
  </div>
</MenuSheet>

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
