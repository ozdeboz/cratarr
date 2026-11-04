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
  import ChoiceRow from './ChoiceRow.svelte';
  import DeviceListening from './DeviceListening.svelte';
  import { readWay, scrobbling, setWay, type Way } from '../scrobbles.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  void loadMine();
  let deleting = $state(false);
  // Where it is scrobbled from (decision 0091; the owner, 2026-10-31: "This also makes more sense privacy wise"): each
  // device, with its own sign-ins, or through the server, signed in to once. A change asked once more, as it lets keys
  // go: to each device, the server sends what it holds, then forgets its keys; back, they are signed in to again there.
  void readWay();
  const WAYS: { value: Way; label: string; line: string }[] = [
    { value: 'device', label: 'Each device', line: "Each signs in itself; Cratarr's server sends nothing out" },
    { value: 'server', label: 'Through the server', line: 'Signed in to once, there, for all your devices' },
  ];
  let switching = $state<Way | null>(null);
</script>

{#if thing}
  {#if scrobbling.way === 'device'}<DeviceListening {thing} />{:else}<ListeningPart {thing} own part="tracking" />{/if}
{:else}
  <Group
    title="Play history"
    more="Your plays, kept for your history, Insights, your taste profile, and what Discover and Radio learn from. Off: nothing is kept or sent to your listening services; favourites, ratings and exclusions stay."
  >
    <Item name="Keep play history" toggle={{ checked: mine.tracking, onchange: (on) => setTracking(on) }} />
    <Item name="Delete play history" kind="danger" onclick={() => (deleting = true)} />
  </Group>
  {#if mine.tracking}
    <Group title="Scrobbling">
      <ChoiceRow name="Scrobble from" options={WAYS} value={scrobbling.way ?? 'server'} onchange={(w) => w !== scrobbling.way && (switching = w)} />
    </Group>
    {#if scrobbling.way === 'device'}<DeviceListening />{:else if scrobbling.way}<ListeningPart own part="tracking" />{/if}
  {/if}
{/if}
{#if switching}
  {@const w = switching}
  <ConfirmSheet
    title={w === 'device' ? 'Scrobble from each device?' : 'Scrobble through the server?'}
    text={w === 'device'
      ? "Cratarr's server sends what it still holds, then forgets your services' keys. Each device then signs in itself, in its own Tracking. Going back means signing in on the server again."
      : "Your devices stop sending; you sign in once on the server, here, and it scrobbles every device's plays. This device's own sign-ins stay, unused."}
    label={w === 'device' ? 'Each device' : 'Through the server'}
    danger={false}
    onconfirm={() => void setWay(w)}
    onclose={() => (switching = null)}
  />
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
