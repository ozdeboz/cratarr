<script lang="ts">
  // Connections' Home Assistant (the owner, 2026-10-24: its own section, its API key there rather than in
  // Authentication). In the list (`page` off), a row with when it last called; on its page, that and the weather it last
  // told, and its keys (lib/settings/KeyGroup): each acting for its owner, only reading or also in control. Acts at once,
  // not with the Save bar.
  import { getHomeAssistant, getUsers, setHomeAssistantOn, type ApiKey, type KeyScope, type Weather } from '../api';
  import { openThing } from '../panel.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import KeyGroup from './KeyGroup.svelte';
  import { when } from './when';

  let { page = false }: { page?: boolean } = $props();

  const MORE =
    "The cratarr-ha integration: Home Assistant plays on Cratarr's players, reads what plays, and tells Cratarr the weather for Home. It signs in with a key made here, given once in the integration's setup.";
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
  <KeyGroup keys={keys ?? []} name="Home Assistant" purpose="home_assistant" scopes={SCOPES} {owners} onchange={load} />
  {#if error}<Group><Item lead="bad" name={error} /></Group>{/if}
{/if}
