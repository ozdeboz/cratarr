<script lang="ts">
  // A server's own page in This device (decision 0098): where it is and who is signed in, whether it scrobbles itself
  // (decision 0091, amended: "This server scrobbles for me", off at the start; no API says whether a server does, so
  // the person says), and Remove, which forgets its sign-in here (its mode goes; Cratarr's music plays on).
  import { backFromThing } from '../panel.svelte';
  import { KIND_NAMES, forgetServer, keepServer, type ServerSignIn } from '../servers.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  let { server }: { server: ServerSignIn } = $props();
  let removing = $state(false);
  const host = $derived.by(() => {
    try {
      return new URL(server.url).host;
    } catch {
      return server.url;
    }
  });
</script>

<Group>
  <Item name="Address" value={host} />
  <Item name="Kind" value={KIND_NAMES[server.kind]} />
  <Item name="Signed in as" value={server.user || (server.apiKey ? 'An API key' : '')} />
</Group>
<Group>
  <Item
    name="This server scrobbles for me"
    more={server.kind === 'jellyfin'
      ? 'On where this Jellyfin tells ListenBrainz or Last.fm what you play itself (its plugin). Off, this device does. Either way a play is sent once.'
      : 'On where this server tells ListenBrainz or Last.fm what you play itself (Navidrome, Gonic and Airsonic-Advanced can, set up there). Off, this device does. Either way a play is sent once.'}
    toggle={{ checked: !!server.scrobbles, onchange: (on) => keepServer({ ...server, scrobbles: on }) }}
  />
</Group>
<Group>
  <Item name={`Remove ${server.name}`} kind="danger" onclick={() => (removing = true)} />
</Group>

{#if removing}
  <ConfirmSheet
    title={`Remove ${server.name}?`}
    label="Remove"
    onconfirm={() => {
      forgetServer(server.id);
      backFromThing('device');
    }}
    onclose={() => (removing = false)}
  />
{/if}
