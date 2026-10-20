<script lang="ts">
  // Reset (decision 0057 §4e): to the playing track's own look (its seed, everything not pinned, as a new track does),
  // or everything back to the Default preset, pins and all.
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';

  let { onseed, oneverything, onclose }: { onseed: () => void; oneverything: () => void; onclose: () => void } = $props();
</script>

<MenuSheet label="Reset" title="Reset" icon="undo" over {onclose}>
  <MenuEntry label="Reset to the track’s look" onclick={() => (onseed(), onclose())} />
  <MenuEntry label="Reset everything" danger onclick={() => (oneverything(), onclose())} />
</MenuSheet>
