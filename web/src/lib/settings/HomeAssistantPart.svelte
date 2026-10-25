<script lang="ts">
  // Connections' Home Assistant (the owner, 2026-10-24: its own section, its API key there rather than in
  // Authentication; no other client needs a key). In the list (`page` off), a row with when it last called; on its page,
  // that and the weather it last told, and its keys: each acting for its owner, only reading or also in control, to
  // revoke; a new one asks for the password again and shows once (decisions 0043, 0051). Acts at once, not with the
  // Save bar.
  import { getHomeAssistant, getUsers, newApiKey, revokeApiKey, setHomeAssistantOn, type ApiKey, type KeyScope, type Weather } from '../api';
  import { copyText } from '../clipboard';
  import { me } from '../me.svelte';
  import { openThing } from '../panel.svelte';
  import Seg from '../Seg.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { when } from './when';

  let { page = false }: { page?: boolean } = $props();

  const MORE =
    "The cratarr-ha integration: Home Assistant plays on Cratarr's players, reads what plays, and tells Cratarr the weather for Home. It signs in with a key made here, given once in the integration's setup.";
  const KEY_MORE = 'A key shows once, when it is made; revoking one ends it at once.';
  const SCOPES: { value: KeyScope; label: string }[] = [
    { value: 'control', label: 'Control' },
    { value: 'read', label: 'Read only' },
  ];
  // It tells the weather on each change and every half hour: heard within the hour, it is there.
  const LIVELY = 3600;

  let keys = $state<ApiKey[] | null>(null);
  // Switched on; off, its keys are kept and refused.
  let on = $state(true);
  let weather = $state<Weather | null>(null);
  let owners = $state<string[]>([]);
  let error = $state<string | null>(null);

  const load = () => {
    getHomeAssistant().then(
      (r) => ((keys = r.keys), (weather = r.weather), (on = r.on ?? true)),
      () => (keys = []),
    );
    if (page)
      getUsers().then(
        (u) => (owners = u.filter((x) => x.active).map((x) => x.name)),
        () => (owners = []),
      );
  };
  load();

  const heard = $derived(Math.max(0, ...(keys ?? []).map((k) => k.last_used ?? 0), weather?.at ?? 0));
  const status = $derived.by(() => {
    if (keys === null) return null;
    if (!on) return { lead: 'off', line: 'Off' };
    if (!keys.length) return { lead: 'off', line: 'No key yet' };
    if (!heard) return { lead: 'off', line: 'Not heard from yet' };
    const lively = Date.now() / 1000 - heard < LIVELY;
    return { lead: lively ? 'on' : 'bad', line: `Last heard ${when(heard)}` };
  });

  const CONDITIONS: Record<string, string> = {
    sunny: 'Sunny',
    'clear-night': 'Clear',
    partlycloudy: 'Partly cloudy',
    cloudy: 'Cloudy',
    fog: 'Fog',
    rainy: 'Rain',
    pouring: 'Pouring',
    lightning: 'Thunder',
    'lightning-rainy': 'Thunderstorm',
    snowy: 'Snow',
    'snowy-rainy': 'Sleet',
    hail: 'Hail',
    windy: 'Windy',
    'windy-variant': 'Windy',
    exceptional: 'Exceptional',
  };
  const weatherLine = (w: Weather) =>
    [w.condition ? (CONDITIONS[w.condition] ?? w.condition) : null, w.temperature != null ? `${Math.round(w.temperature)} °C` : null, when(w.at)]
      .filter(Boolean)
      .join(' · ');

  const failed = (e: unknown) => (error = e instanceof Error ? e.message : String(e));

  async function switchOn(to: boolean) {
    error = null;
    on = to;
    try {
      await setHomeAssistantOn(to);
    } catch (e) {
      on = !to;
      failed(e);
    }
  }

  // A new key: whom it acts for (oneself unless chosen), what it may do, and the password; shown here once.
  let making = $state(false);
  let busy = $state(false);
  let keyOwner = $state('');
  let keyScope = $state<KeyScope>('control');
  let keyPassword = $state('');
  let made = $state<{ id: string; key: string } | null>(null);
  async function makeKey() {
    error = null;
    busy = true;
    try {
      const k = await newApiKey('Home Assistant', keyPassword, keyOwner || undefined, keyScope, 'home_assistant');
      made = { id: k.id, key: k.key };
      making = false;
      load();
    } catch (e) {
      failed(e);
    } finally {
      keyPassword = '';
      busy = false;
    }
  }

  async function revoke(k: ApiKey) {
    error = null;
    try {
      await revokeApiKey(k.id);
      if (made?.id === k.id) made = null;
      load();
    } catch (e) {
      failed(e);
    }
  }

  const keyLine = (k: ApiKey) =>
    [k.owner, k.scope === 'read' ? 'Read only' : null, `Made ${when(k.created)}`, k.last_used ? `used ${when(k.last_used)}` : 'not used yet'].filter(Boolean).join(' · ');
</script>

{#if !page}
  <Group title="Home automation" more={MORE}>
    {#if status}
      <Item lead={status.lead} name="Home Assistant" line={status.line} tone={status.lead === 'bad' ? 'bad' : null} onclick={() => openThing('connections', 'home-assistant')} />
    {/if}
  </Group>
{:else}
  <Group>
    <Item name="Use Home Assistant" line={on ? null : 'Its keys are kept, and refused until it is on again'} toggle={{ checked: on, onchange: switchOn }} />
  </Group>
  <Group title="Connection" more={MORE}>
    {#if status}
      <Item lead={status.lead} name={status.lead === 'on' ? 'Connected' : 'Not connected'} line={status.line} tone={status.lead === 'bad' ? 'bad' : null} />
    {/if}
    <Item name="Weather" line={weather ? weatherLine(weather) : 'Not told yet'} />
  </Group>
  <Group title="API key" more={KEY_MORE}>
    {#each keys ?? [] as k (k.id)}
      {#if made?.id === k.id}
        {@const m = made}
        <Item lead="on" name="Shown only now" line={m.key} wrap>
          <button onclick={() => copyText(m.key)}>Copy</button>
        </Item>
      {:else}
        <Item name={k.name} line={keyLine(k)}>
          <button onclick={() => revoke(k)}>Revoke</button>
        </Item>
      {/if}
    {/each}
    <Item name="New key" kind="add" onclick={() => ((error = null), (making = true))} />
  </Group>
  {#if error && !making}<Group><Item lead="bad" name={error} /></Group>{/if}
  {#if making}
    <FormSheet title="New key" submitLabel="Make key" canSubmit={!!keyPassword} {busy} {error} onsubmit={makeKey} onclose={() => ((making = false), (keyPassword = ''), (error = null))}>
      {#if owners.length > 1}
        <label
          >Acts for<select bind:value={keyOwner}>
            {#each owners as o (o)}<option value={o === me.name ? '' : o}>{o}</option>{/each}
          </select></label
        >
      {/if}
      <span class="scope"><Seg label="Scope" options={SCOPES} bind:value={keyScope} /></span>
      <!-- svelte-ignore a11y_autofocus -->
      <label>Password<input type="password" bind:value={keyPassword} autocomplete="current-password" autofocus /></label>
    </FormSheet>
  {/if}
{/if}

<style>
  .scope {
    display: flex;
  }
</style>
