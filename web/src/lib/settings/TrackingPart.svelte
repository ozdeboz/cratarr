<script lang="ts">
  // Tracking (the owner, 2026-10-25: a Personal part): whether Cratarr keeps one's plays at all. Off is all of it (the
  // owner's choice): no plays kept, nothing sent to listening services, no taste profile, nothing learned from plays;
  // favourites, ratings and exclusions stay (things one said, not tracking). What is kept stays until deleted (asked
  // once more). Then one's listening services (decision 0068), each person their own, moved here from Connections.
  import { deleteHistory } from '../api';
  import { loadMine, mine, setTracking } from '../mySettings.svelte';
  import { toast } from '../toast.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import ListeningPart from './ListeningPart.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  void loadMine();
  let deleting = $state(false);
</script>

{#if thing}
  <ListeningPart {thing} own part="tracking" />
{:else}
  <Group
    title="Play history"
    more="Your plays, kept for your history, Insights, your taste profile, and what Discover and Radio learn from. Off: nothing is kept or sent to your listening services; favourites, ratings and exclusions stay."
  >
    <Item name="Keep play history" toggle={{ checked: mine.tracking, onchange: (on) => setTracking(on) }} />
    <Item name="Delete play history" kind="danger" onclick={() => (deleting = true)} />
  </Group>
  {#if mine.tracking}<ListeningPart own part="tracking" />{/if}
{/if}
{#if deleting}
  <ConfirmSheet
    title="Delete play history?"
    text="Every play kept for you, and what was learned from them. Favourites, ratings and exclusions stay."
    label="Delete"
    onconfirm={() => deleteHistory().catch((e) => toast.fail(e, "The history wasn't deleted"))}
    onclose={() => (deleting = false)}
  />
{/if}
