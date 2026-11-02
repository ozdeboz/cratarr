<script lang="ts">
  // A long press on the player's info button (the owner, 2026-10-26: only the controls that concern it): which facts
  // show beside it, the same rows as Settings → Player's Details (lib/settings/DetailsGroup), the reset in the header
  // (the owner, 2026-10-26).
  import MenuSheet from './MenuSheet.svelte';
  import DetailsGroup from './settings/DetailsGroup.svelte';
  import { resetOf, rowsOf } from './settings/reset.svelte';
  import { DETAILS, playerItems } from './settings/resetItems.svelte';
  import ResetSheet from './settings/ResetSheet.svelte';

  let { onclose }: { onclose: () => void } = $props();
  let asking = $state(false);
  // Settings → Player's Details section, as its Reset there (lib/settings/resetItems).
  const rows = $derived(rowsOf(playerItems(), DETAILS));
</script>

<MenuSheet
  label="Extra details"
  title="Extra details"
  end={{ icon: 'reset', label: 'Reset extra details', disabled: !rows.length, onclick: () => (asking = true) }}
  {onclose}
>
  <div class="rows">
    <DetailsGroup toggle={false} />
  </div>
</MenuSheet>
{#if asking && rows.length}
  <ResetSheet title="Reset extra details" {rows} onconfirm={() => resetOf(playerItems(), DETAILS)} onclose={() => (asking = false)} />
{/if}

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
