<script lang="ts">
  // What is excluded, in a sheet (decision 0024, amended 2026-10-19): a crate's, from the "N excluded" line under its
  // name in the panel (any crate's: one's own, Unlisted, a genre's). The global ones are on Settings › Taste since 0.8.
  // The list is lib/ExcludedList; the sheet closes once all of it is back.
  import { crateName } from './crates.svelte';
  import ExcludedList from './ExcludedList.svelte';
  import MenuSheet from './MenuSheet.svelte';

  let { crate = null, onchange = () => {}, onclose }: { crate?: string | null; onchange?: () => void; onclose: () => void } = $props();
</script>

<MenuSheet label="Excluded" icon={crate ? 'crateOut' : 'ban'} title="Excluded" chips={crate ? [crateName(crate)] : []} {onclose}>
  <ExcludedList {crate} {onchange} onempty={onclose} />
</MenuSheet>
