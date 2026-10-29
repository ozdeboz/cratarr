<script lang="ts">
  // The account's Spotify (decision 0073; the owner, 2026-10-27), Account › Spotify: what of it the Library shows, and
  // the two ways it comes. Spotify's export, anyone's: uploaded in Import, a snapshot until the next. The live
  // connection, Premium's: the person's own Spotify app (its Client ID; the server has none of its own), Spotify's
  // sign-in coming back to this page, read again every 6 hours. Live wins; disconnected, the export shows again. No
  // playing on Spotify: what is here plays from the library, what is not is asked for.
  import { connectSpotify, disconnectSpotify, forgetSpotifyImport, setSpotifyClient, syncSpotify } from '../api';
  import { copyText } from '../clipboard';
  import { openThing } from '../panel.svelte';
  import { prefs } from '../prefs.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import { loadSpotify, spotify } from '../spotify.svelte';
  import { toast } from '../toast.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';
  import { when } from './when';

  nameThing(() => 'Spotify');

  const EXPORT_MORE =
    "Anyone's, free or Premium. In Spotify: your account › Privacy settings › Download your data, Account data. Spotify emails a .zip, usually within a few days; upload it in Import › Files. A newer one brings it up to date.";
  const LIVE_MORE =
    'Premium only, with your own Spotify app: at developer.spotify.com, Dashboard › Create app (any name), add the address below as its Redirect URI, and give its Client ID here. It is your app, so you need not add yourself as its user. Read again every 6 hours.';

  const sp = $derived(spotify.state);
  const callback = `${location.origin}/api/spotify/callback`;
  // Spotify takes an https address only (a loopback one aside): the LAN's http one cannot connect.
  const loopback = ['localhost', '127.0.0.1', '[::1]'].includes(location.hostname);
  const secure = location.protocol === 'https:' || loopback;

  let error = $state<string | null>(new URLSearchParams(location.hash.split('?')[1] ?? '').get('spotify_error'));
  const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

  // Read again while it is being read (the first read on connecting, a Refresh).
  $effect(() => {
    if (sp?.live?.sync.state !== 'running') return;
    const t = setTimeout(() => loadSpotify(true), 3000);
    return () => clearTimeout(t);
  });

  const n = (x: number) => x.toLocaleString();
  const counts = $derived(sp?.shown ? `${n(sp.counts.albums)} albums, ${n(sp.counts.tracks)} tracks, ${n(sp.counts.playlists)} playlists, ${n(sp.counts.artists)} artists` : null);
  const syncLine = $derived.by(() => {
    const s = sp?.live?.sync;
    if (!s) return null;
    if (s.state === 'running') return 'Reading your library';
    if (s.state === 'failed') return s.error ?? 'The last read failed';
    return s.at ? `Read ${when(s.at)}` : null;
  });

  function showInDiscover() {
    prefs.discoverMode = 'spotify-albums';
    location.hash = '#/discover';
  }

  let typing = $state(false);
  async function giveClient(id: string) {
    error = null;
    try {
      await setSpotifyClient(id.trim());
      await loadSpotify(true);
    } catch (e) {
      error = said(e);
      throw e;
    }
  }
  let connecting = $state(false);
  async function connect() {
    error = null;
    connecting = true;
    try {
      // In the Android app, Spotify opens in the browser, and its way back ends in the app (cratarr://), as Last.fm's.
      const { url } = await connectSpotify(callback, navigator.userAgent.includes('CratarrApp/'));
      location.assign(url);
    } catch (e) {
      error = said(e);
      connecting = false;
    }
  }
  async function refresh() {
    error = null;
    try {
      spotify.state = await syncSpotify();
    } catch (e) {
      error = said(e);
    }
  }
  async function copy() {
    if (!(await copyText(callback))) toast.fail(null, "It couldn't be copied");
  }

  let forgetting = $state<'live' | 'export' | null>(null);
  async function forget(what: 'live' | 'export') {
    error = null;
    try {
      await (what === 'live' ? disconnectSpotify() : forgetSpotifyImport());
      await loadSpotify(true);
    } catch (e) {
      error = said(e);
    }
  }
</script>

<Group title="In the Library">
  <Item
    lead={sp?.shown ? 'on' : 'off'}
    name={counts ?? 'Nothing yet'}
    line={sp?.shown === 'live' ? 'From your live connection' : sp?.shown === 'imported' ? 'From your export' : 'Upload your export, or connect'}
    wrap
  />
  {#if sp?.shown}<Item name="Show in Discover" onclick={showInDiscover} />{/if}
  {#if error}<Item lead="bad" name={error} wrap />{/if}
</Group>

<Group title="Spotify's export" more={EXPORT_MORE}>
  <Item name={sp?.imported ? `Uploaded ${when(sp.imported.at)}` : 'Not uploaded'} line={sp?.imported?.file ?? null} wrap onclick={() => openThing('import', 'files')} />
  {#if sp?.imported}<Item name="Forget the export's library" kind="danger" onclick={() => (forgetting = 'export')} />{/if}
</Group>

<Group title="Live connection" more={LIVE_MORE}>
  <Item name="Client ID" line={sp?.client_id ? 'Given' : 'Not given'} tone={sp?.client_id ? null : 'warn'} onclick={() => (typing = true)} />
  <Item name="Redirect URI" line={callback} wrap>
    <button onclick={copy}>Copy</button>
  </Item>
  {#if !secure}<Item lead="bad" name="Connect from Cratarr's https address" line="Spotify refuses this one" tone="warn" wrap />{/if}
  {#if sp?.live}
    <Item lead={sp.live.sync.state === 'failed' ? 'bad' : 'on'} name={`Connected as ${sp.live.name}`} line={syncLine} tone={sp.live.sync.state === 'failed' ? 'bad' : null} wrap>
      <button disabled={sp.live.sync.state === 'running'} aria-busy={sp.live.sync.state === 'running'} onclick={refresh}>Refresh</button>
    </Item>
  {:else}
    <Item name="Not connected" line={sp?.client_id ? null : 'Give the Client ID first'}>
      <button class="primary" disabled={!sp?.client_id || !secure || connecting} aria-busy={connecting} onclick={connect}>Connect</button>
    </Item>
  {/if}
</Group>

{#if sp?.live}
  <Group>
    <Item name="Disconnect Spotify" kind="danger" onclick={() => (forgetting = 'live')} />
  </Group>
{/if}

{#if typing}
  <ValueSheet title="Client ID" value={sp?.client_id ?? ''} help="Your app's, on its page at developer.spotify.com. Empty forgets it, and disconnects." onsave={giveClient} onclose={() => (typing = false)} />
{/if}
{#if forgetting}
  {@const what = forgetting}
  <ConfirmSheet
    title={what === 'live' ? 'Disconnect Spotify?' : "Forget the export's library?"}
    text={what === 'live' ? 'Its lists go from the Library; an export you uploaded shows again.' : 'Its lists go from the Library; its listens stay in your history.'}
    label={what === 'live' ? 'Disconnect' : 'Forget'}
    onconfirm={() => forget(what)}
    onclose={() => (forgetting = null)}
  />
{/if}
