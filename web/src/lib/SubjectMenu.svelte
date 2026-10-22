<script lang="ts">
  // The menu of what a badge opens (decision 0058: a badge, chip or link that opens an object opens that object's menu on
  // a long-press or a right-click), by the subject it views: a crate's or a genre's (`crate:…`), a playlist's (its id).
  import CrateMenu from './CrateMenu.svelte';
  import PlaylistMenu from './PlaylistMenu.svelte';
  import { crateOf } from './crates.svelte';
  import { shownPlaylists } from './output.svelte';

  let { subject, onclose }: { subject: string; onclose: () => void } = $props();

  const crate = $derived(crateOf(subject));
  const playlist = $derived(crate === null ? (shownPlaylists().find((p) => p.id === subject) ?? null) : null);
</script>

{#if crate !== null}
  <CrateMenu id={crate} {onclose} />
{:else if playlist}
  <PlaylistMenu playlists={[playlist]} {onclose} />
{/if}
