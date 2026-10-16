<script lang="ts">
  // In select mode, a long-press on an item (the owner, 2026-10-16): what to tick among what the list shows: all of
  // it, none of it, or the others (what is ticked unticked, the rest ticked). The same on every list with a select
  // mode (the Library, Playlists, a list's tracks).
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';

  let { onall, onnone, onothers, onclose }: { onall: () => void; onnone: () => void; onothers: () => void; onclose: () => void } = $props();

  const pick = (what: () => void) => () => {
    what();
    onclose();
  };
</script>

<MenuSheet label="Select" title="Select" icon="listChecks" {onclose}>
  <div role="menu">
    <MenuEntry icon="circleCheck" label="Select all" onclick={pick(onall)} />
    <MenuEntry icon="circle" label="Select none" onclick={pick(onnone)} />
    <MenuEntry icon="swap" label="Select others" onclick={pick(onothers)} />
  </div>
</MenuSheet>
