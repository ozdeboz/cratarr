<script lang="ts">
  // A change to many at once, asked once more (lib/many): App's, one for every list.
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import { manyAsk } from './many.svelte';
</script>

{#if manyAsk.value}
  {@const a = manyAsk.value}
  <ConfirmSheet title={a.title} label={a.label} icon={a.icon} onconfirm={a.go} onclose={() => (manyAsk.value = null)} />
{/if}
