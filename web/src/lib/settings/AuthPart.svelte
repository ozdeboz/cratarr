<script lang="ts">
  // Authentication (the owner, 2026-10-16: not "Subsonic"), its administrators' since several users (decision 0051):
  // whether every request needs a sign-in (decision 0043); the directory. Switching the requirement asks for the
  // password again, in a sheet (the settings' North Star, 2026-10-19). Acts at once. API keys are with what uses them,
  // Home Assistant's under Connections (the owner, 2026-10-24). The accounts are all in Users (0051, amended
  // 2026-10-19: Cratarr's fallback administrator is one of them); each one's own password and sign-ins are under
  // Account, the address apps take too.
  import { loadRuntime, runtime, saveRuntime } from '../serverRuntime.svelte';
  import ValueSheet from './ValueSheet.svelte';
  import { getSignInRequired, setSignInRequired } from '../api';
  import DirectoryPart from './DirectoryPart.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { me } from '../me.svelte';
  import { playOn } from '../playOn.svelte';

  const REQUIRED_MORE =
    'On, every page and API request needs a sign-in, an API key or a signed link, on the home network too. Off, Cratarr asks none itself: a proxy in front of it may. Subsonic apps always sign in with their name and password.\n\nSwitching it asks for the password again.';

  let required = $state<boolean | null>(null);
  let error = $state<string | null>(null);

  getSignInRequired().then(
    (r) => (required = r.required),
    () => (required = null),
  );

  const failed = (e: unknown) => (error = e instanceof Error ? e.message : String(e));

  // The proxies whose forwarded address is believed (lib/serverRuntime).
  loadRuntime();
  let editingProxies = $state(false);
  const PROXIES_MORE =
    "A proxy in front of Cratarr tells it the address each request came from. Cratarr believes that from the proxies named here: addresses or ranges. None named: a proxy on a local address (this computer, its Docker network, your home network), never one on the internet.";
  const proxyList = (v: string) => v.split(/[\s,]+/).map((x) => x.trim()).filter(Boolean);
  // Home (0.9.18-1; the owner, 2026-10-30): where other users may play on, follow or control a device someone shared.
  let editingHome = $state(false);
  // The guest page's host (0.10): a user, or one of the players.
  let choosingHost = $state(false);
  const hostName = (h: { user: string } | { player: string } | null) => (!h ? 'No one' : 'user' in h ? h.user : h.player);
  async function pickHost(host: { user: string } | { player: string }) {
    const g = runtime.value?.guest;
    if (!g) return;
    if (await saveRuntime({ guest: { ...g, host } })) choosingHost = false;
  }
  const HOME_MORE =
    "Other users may play on, follow or control a device someone shared only from home: a local address, or one named here. Name your home's public address if devices at home reach Cratarr by its public name. Never a VPN's.";

  // The requirement: flipping it asks for the password, then switches.
  let switching = $state<boolean | null>(null);
  let switchPassword = $state('');
  let busy = $state(false);
  async function confirmSwitch() {
    if (switching === null) return;
    error = null;
    busy = true;
    try {
      required = (await setSignInRequired(switching, switchPassword)).required;
      switching = null;
    } catch (e) {
      failed(e);
    } finally {
      switchPassword = '';
      busy = false;
    }
  }
</script>

{#if required !== null}
  <Group title="Sign-in">
    <Item
      name="Sign-in required everywhere"
      more={REQUIRED_MORE}
      disabled={busy}
      toggle={{ checked: switching ?? required, onchange: (on) => (switching = on === required ? null : on) }}
    />
  </Group>
{/if}

<!-- The proxies whose forwarded client address is believed (0.9.8: the environment's until then; lib/serverRuntime). -->
{#if runtime.value}
  {@const r = runtime.value}
  <Group title="Proxies" more={PROXIES_MORE}>
    <Item
      name="Trusted proxies"
      line={r.from_env?.proxies ? 'Set by the environment until changed here' : null}
      value={r.proxies.length ? r.proxies.join(', ') : 'Local addresses'}
      onclick={() => (editingProxies = true)}
    />
    {#if runtime.error}<Item lead="bad" name={runtime.error} />{/if}
  </Group>
  {#if r.home_networks}
    <Group title="Home" more={HOME_MORE}>
      <Item name="Home networks" value={r.home_networks.length ? r.home_networks.join(', ') : 'Local addresses'} onclick={() => (editingHome = true)} />
      {#if r.guest}
        {@const g = r.guest}
        <!-- The guest page (0.10): /guest on the home network, its tracks asked of the host's player. -->
        <Item name="Guest page" line={g.on ? `${location.origin}/guest` : null} toggle={{ checked: g.on, onchange: (on) => void saveRuntime({ guest: { ...g, on } }) }} />
        {#if g.on}<Item name="Guests ask" value={hostName(g.host)} onclick={() => (choosingHost = true)} />{/if}
      {/if}
    </Group>
  {/if}
{/if}

{#if error && switching === null}<Group><Item lead="bad" name={error} /></Group>{/if}
<DirectoryPart />

{#if editingProxies}
  <ValueSheet
    title="Trusted proxies"
    value={runtime.value?.proxies.join(', ') ?? ''}
    placeholder="Local addresses"
    help="Addresses or ranges, separated by commas: 172.19.0.0/16, 192.168.1.5"
    onsave={async (v) => {
      if (await saveRuntime({ proxies: proxyList(v) })) editingProxies = false;
    }}
    onclose={() => (editingProxies = false)}
  />
{/if}

{#if choosingHost && runtime.value?.guest}
  {@const g = runtime.value.guest}
  <!-- Whose player the guests' tracks are asked of: oneself (whatever one plays on), or one player. -->
  <MenuSheet label="Guests ask" title="Guests ask" step="menu" onclose={() => (choosingHost = false)}>
    <div role="menu">
      {#if me.name}
        {@const name = me.name}
        <MenuEntry label={`Me (${name})`} checked={!!g.host && 'user' in g.host && g.host.user === name} plus={false} onclick={() => pickHost({ user: name })} />
      {/if}
      {#each playOn.players.filter((p) => p.mine !== false || p.shared) as p (p.name)}
        <MenuEntry label={p.name} checked={!!g.host && 'player' in g.host && g.host.player === p.name} plus={false} onclick={() => pickHost({ player: p.name })} />
      {/each}
    </div>
  </MenuSheet>
{/if}

{#if editingHome}
  <ValueSheet
    title="Home networks"
    value={runtime.value?.home_networks?.join(', ') ?? ''}
    placeholder="Local addresses"
    help="Addresses or ranges besides the local ones, separated by commas: 203.0.113.7, 2001:db8::/56"
    onsave={async (v) => {
      if (await saveRuntime({ home_networks: proxyList(v) })) editingHome = false;
    }}
    onclose={() => (editingHome = false)}
  />
{/if}

{#if switching !== null}
  <FormSheet
    title={switching ? 'Sign-in required everywhere' : 'Sign-in not required'}
    submitLabel={switching ? 'Switch on' : 'Switch off'}
    canSubmit={!!switchPassword}
    {busy}
    {error}
    onsubmit={confirmSwitch}
    onclose={() => ((switching = null), (switchPassword = ''), (error = null))}
  >
    <!-- svelte-ignore a11y_autofocus -->
    <label>Password<input type="password" bind:value={switchPassword} autocomplete="current-password" autofocus /></label>
  </FormSheet>
{/if}

