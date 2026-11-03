<script lang="ts">
  // This device's listening services (decision 0091, from each device): ListenBrainz and Last.fm signed in to here, kept
  // on this device only (the app's own store, a browser's), never sent to Cratarr's server. As the server's (lib/
  // settings/ListeningPart): a row each with its state, its own page behind it (`thing`) with its sign-in and Disconnect.
  import { backFromThing, openThing } from '../panel.svelte';
  import { forgetHere, keepListenBrainz, lastFmBack, readHere, scrobbling, signInLastFm } from '../scrobbles.svelte';
  import type { Service } from '../scrobbleDevice';
  import { nameThing } from '../settingsSections.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';

  let { thing = null }: { thing?: string | null } = $props();
  const NAMES = { listenbrainz: 'ListenBrainz', lastfm: 'Last.fm' } as const;
  const MORE =
    "Every play this device sounds, from Cratarr and from every server you play here, sent from this device to the services you sign in to here, with what starts as now playing. The sign-ins stay on this device: Cratarr's server never has them.\n\nA service that is down gets them later; one that refuses the sign-in stops until you sign in again.";
  const service = $derived(thing === 'listenbrainz' || thing === 'lastfm' ? (thing as Service) : null);
  nameThing(() => (service ? NAMES[service] : null));

  readHere();
  let error = $state<string | null>(null);
  // Back from Last.fm's sign-in (a browser): its token traded for the session.
  lastFmBack().then((e) => e && (error = e));

  const lineOf = (s: Service) => {
    const h = scrobbling.here[s];
    if (!h) return 'Not signed in on this device';
    if (h.broken) return h.broken;
    return `Sending as ${h.name}${scrobbling.queued ? ` · ${scrobbling.queued.toLocaleString()} waiting` : ''}`;
  };

  let typing = $state<'token' | 'key' | 'secret' | null>(null);
  let apiKey = $state('');
  let apiSecret = $state('');
  let forgetting = $state(false);
  async function giveToken(v: string) {
    error = null;
    try {
      await keepListenBrainz(v.trim());
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

{#if !service}
  <Group title="This device's listening services" more={MORE}>
    {#each Object.entries(NAMES) as [id, name] (id)}
      {@const s = id as Service}
      {@const h = scrobbling.here[s]}
      {#if h}
        <Item lead={h.broken ? 'bad' : 'on'} {name} line={lineOf(s)} tone={h.broken ? 'bad' : null} onclick={() => openThing('tracking', s)} />
      {:else}
        <Item name={`Add ${name}`} kind="add" onclick={() => openThing('tracking', s)} />
      {/if}
    {/each}
  </Group>
{:else}
  {@const h = scrobbling.here[service]}
  <Group title="On this device" more={MORE}>
    <Item lead={h && !h.broken ? 'on' : h ? 'bad' : 'off'} name={h && !h.broken ? 'Signed in' : 'Not signed in'} line={lineOf(service)} tone={h?.broken ? 'bad' : null} wrap />
    {#if error}<Item lead="bad" name={error} wrap />{/if}
  </Group>
  {#if service === 'listenbrainz'}
    <Group title="Sign-in">
      <Item name="User token" line={h ? 'Given' : 'On listenbrainz.org, under your name › Settings'} tone={h ? null : 'warn'} onclick={() => (typing = 'token')} />
    </Group>
  {:else}
    <Group title="Sign-in" more="Make both at last.fm/api/account/create (any name will do). Then Last.fm asks you to allow Cratarr, and brings you back here.">
      <Item name="API key" line={apiKey ? 'Given' : h ? 'Kept' : 'Not given'} tone={apiKey || h ? null : 'warn'} onclick={() => (typing = 'key')} />
      <Item name="Shared secret" line={apiSecret ? 'Given' : h ? 'Kept' : 'Not given'} tone={apiSecret || h ? null : 'warn'} onclick={() => (typing = 'secret')} />
      <Item name={h && !h.broken ? 'Signed in at Last.fm' : 'Not signed in at Last.fm'} line={apiKey && apiSecret ? null : 'Give the key and the secret first'}>
        <button class="primary" disabled={!apiKey.trim() || !apiSecret.trim()} onclick={() => signInLastFm(apiKey.trim(), apiSecret.trim())}>Sign in</button>
      </Item>
    </Group>
  {/if}
  {#if h}
    <Group>
      <Item name={`Disconnect ${NAMES[service]}`} kind="danger" onclick={() => (forgetting = true)} />
    </Group>
  {/if}

  {#if typing === 'token'}
    <ValueSheet title="User token" type="password" value="" help="On listenbrainz.org, under your name › Settings." allowEmpty={false} onsave={giveToken} onclose={() => (typing = null)} />
  {:else if typing === 'key'}
    <ValueSheet title="API key" value={apiKey} help="Made at last.fm/api/account/create." allowEmpty={false} onsave={(v) => void (apiKey = v)} onclose={() => (typing = null)} />
  {:else if typing === 'secret'}
    <ValueSheet title="Shared secret" type="password" value="" help="Shown beside the API key at last.fm/api/accounts." allowEmpty={false} onsave={(v) => void (apiSecret = v)} onclose={() => (typing = null)} />
  {/if}
  {#if forgetting}
    {@const s = service}
    <ConfirmSheet
      title={`Disconnect ${NAMES[s]} here?`}
      text="Plays on this device are no longer sent there; those still waiting are dropped."
      label="Disconnect"
      onconfirm={() => (forgetHere(s), backFromThing('tracking'))}
      onclose={() => (forgetting = false)}
    />
  {/if}
{/if}
