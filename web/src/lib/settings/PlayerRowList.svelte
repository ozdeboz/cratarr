<script lang="ts">
  // The player's row (the owner, 2026-10-19): which facts show beside its info button, in the user's order
  // (lib/settings/OrderList). A fact a track has no value for is left out there.
  import { FACTS } from '../facts';
  import { prefs } from '../prefs.svelte';
  import OrderList from './OrderList.svelte';
</script>

<OrderList items={FACTS} on={prefs.playerFacts} label="Facts beside the info button" onchange={(v) => (prefs.playerFacts = v)} />
