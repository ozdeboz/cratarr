<script lang="ts">
  // Settings → Devices (the owner, 2026-10-25: by kind, each its own page): the apps, the Kodis (lib/KodiSettings), the
  // browsers that played; each a row opening its page (`thing`: `device:<name>`, a Kodi's `kodi:<n>`), there what
  // it is, when it was last seen, its speaker delay and Remove. The speaker delay (sync play, decision 0037): how late
  // its speaker sounds, so a group sounds as one; set from any device (standing between two speakers), heard at once
  // while the finger moves, as Snapweb and Iris set Snapcast's per-client latency. 0 to 500 ms in steps of 5, its slider
  // in a sheet (the settings' North Star, 2026-10-19). Shared with everyone (0.9.8, the owner): others on this Cratarr
  // play on it with Play On as its owner does, nothing more; only one's own devices are here.
  import { loadRuntime, runtime, saveRuntime } from './serverRuntime.svelte';
  import ValueSheet from './settings/ValueSheet.svelte';
  import { panSafe } from './panSafe';
  import { removeMyDevice, setPlayerDelay, setPlayerShared, type PlayOnPlayer } from './api';
  import { backFromThing, openThing } from './panel.svelte';
  import { nameThing } from './settingsSections.svelte';
  import { when } from './settings/when';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import Icon from './Icon.svelte';
  import KodiSettings from './KodiSettings.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { playOn } from './playOn.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import { toast } from './toast.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  const MAX = 500;
  /** A Kodi's delay is its own buffering too: up to the group's whole buffer. */
  const KODI_MAX = 1000;
  const maxOf = (name: string) => (playOn.players.find((p) => p.name === name)?.kind === 'kodi' ? KODI_MAX : MAX);
  const STEP = 5;
  const DELAY_MORE =
    "How late each player's speaker sounds. In a group, a player whose speaker sounds late (Bluetooth, a soundbar's processing) plays that much earlier, so the group sounds as one.\n\nStand between two speakers and move it until they sound together. A Kodi's counts its own buffering too, up to a second.";

  const byName = (a: PlayOnPlayer, b: PlayOnPlayer) => a.name.toLowerCase().localeCompare(b.name.toLowerCase());
  /** One's own apps (an app from before kinds says none), and browsers that played: each kind its group. Another's
   *  shared device is not one's own to set. */
  const own = $derived(playOn.players.filter((p) => p.mine !== false));
  const apps = $derived(own.filter((p) => !p.kind || p.kind === 'app').sort(byName));
  const browsers = $derived(own.filter((p) => p.kind === 'browser').sort(byName));
  const KIND = { app: 'App', kodi: 'Kodi', browser: 'Browser' } as const;
  const seen = (p: PlayOnPlayer) => (p.online ? 'Online' : p.seen ? `Seen ${when(p.seen)}` : 'Offline');

  // A device's own page.
  const DEVICE = 'device:';
  const shown = $derived(thing?.startsWith(DEVICE) ? (own.find((p) => p.name === thing.slice(DEVICE.length)) ?? null) : null);
  nameThing(() => shown?.name ?? null);
  // A device gone (removed, or not there once known): back to the list.
  $effect(() => {
    if (thing?.startsWith(DEVICE) && playOn.known && !shown) backFromThing('players');
  });

  // While it moves, its value here; the player list takes over after.
  let held = $state<Record<string, number>>({});
  const delay = (name: string) => held[name] ?? playOn.players.find((p) => p.name === name)?.delay_ms ?? 0;

  const busy = new Map<string, ReturnType<typeof setTimeout>>();
  const wanted = new Map<string, number>();
  const letGo = new Map<string, ReturnType<typeof setTimeout>>();

  /** At most one save per player every 100 ms while it moves; the last always goes. */
  function set(name: string, ms: number) {
    const v = Math.min(maxOf(name), Math.max(0, Math.round(ms / STEP) * STEP));
    held[name] = v;
    wanted.set(name, v);
    clearTimeout(letGo.get(name));
    letGo.set(
      name,
      setTimeout(() => delete held[name], 3000),
    );
    if (busy.has(name)) return;
    const go = () => {
      const l = wanted.get(name) ?? v;
      setPlayerDelay(name, l).catch((e) => toast.fail(e, "The delay wasn't saved"));
      busy.set(
        name,
        setTimeout(() => {
          busy.delete(name);
          if (wanted.get(name) !== l) go();
        }, 100),
      );
    };
    go();
  }

  // Shared with everyone: as saved here until the player list says so.
  let sharing = $state<Record<string, boolean>>({});
  const isShared = (p: PlayOnPlayer) => sharing[p.name] ?? p.shared === true;
  const SHARED_MORE =
    'Everyone on this Cratarr can play on it with Play On, as you do: what plays, its queue, and groups. Its settings and Remove stay yours.';
  async function share(name: string, on: boolean) {
    sharing[name] = on;
    try {
      sharing[name] = (await setPlayerShared(name, on)).shared;
    } catch (e) {
      delete sharing[name];
      toast.fail(e, on ? `${name} wasn't shared` : `${name} is still shared`);
    }
  }

  // The player whose delay is being set, in its sheet.
  let adjusting = $state<string | null>(null);
  // A player removed from its sheet (the owner, 2026-10-24): one's own device forgotten, its queue with it; not while
  // it is connected (the server says so).
  let removing = $state<string | null>(null);
  async function remove(name: string) {
    try {
      await removeMyDevice(name);
      backFromThing('players');
    } catch (e) {
      toast.fail(e, `${name} wasn't removed`);
    }
  }
  // Playing in step's address (lib/serverRuntime).
  loadRuntime();
  let editingHost = $state(false);
  const STEP_HOST_MORE =
    'Players in a group (and a Kodi box) reach Cratarr at this address on your home network. Empty: the address the page is opened at, which a proxy on a public name may not be.';
</script>

{#snippet row(p: PlayOnPlayer)}
  <Item lead={p.online ? 'on' : 'off'} name={p.name} line={seen(p)} onclick={() => openThing('players', DEVICE + p.name)} />
{/snippet}
{#snippet delayRow(name: string)}
  <Item name="Speaker delay" more={DELAY_MORE} value={`${delay(name)} ms`} onclick={() => (adjusting = name)} />
{/snippet}

{#if shown}
  {@const p = shown}
  <Group>
    <Item name="Kind" value={KIND[p.kind ?? 'app']} />
    <Item name="Last seen" value={p.online ? 'Online' : p.seen ? when(p.seen) : 'Never'} />
  </Group>
  <!-- A browser plays only while it is open, never in a group: no delay. -->
  {#if p.kind !== 'browser'}<Group>{@render delayRow(p.name)}</Group>{/if}
  <Group>
    <Item name="Shared with everyone" more={SHARED_MORE} toggle={{ checked: isShared(p), onchange: (on) => void share(p.name, on) }} />
  </Group>
  <Group>
    <Item name={`Remove ${p.name}`} kind="danger" line={p.online ? 'Close Cratarr there first' : null} disabled={p.online} onclick={() => (removing = p.name)} />
  </Group>
{:else if !thing}
  <Group title="Apps" more="Cratarr's app on a phone or a tablet: a player for Play On and groups, whether Cratarr is open there or not.">
    {#each apps as p (p.name)}{@render row(p)}{:else}{#if playOn.known}<Item name="No apps yet" />{/if}{/each}
  </Group>
{/if}
<KodiSettings {thing} delay={delayRow} />
<!-- Playing in step's address (0.9.8: the environment's until then; lib/serverRuntime). -->
{#if !thing && runtime.value}
  {@const r = runtime.value}
  <Group title="Playing in step" more={STEP_HOST_MORE}>
    <Item
      name="Address on the home network"
      line={r.from_env?.snapcast_host ? 'Set by the environment until changed here' : null}
      value={r.snapcast_host || 'As the page is opened at'}
      onclick={() => (editingHost = true)}
    />
    {#if runtime.error}<Item lead="bad" name={runtime.error} />{/if}
  </Group>
{/if}
{#if editingHost}
  <ValueSheet
    title="Address on the home network"
    value={runtime.value?.snapcast_host ?? ''}
    placeholder="As the page is opened at"
    help="The server's address on your home network, e.g. 192.168.1.5"
    onsave={async (v) => {
      if (await saveRuntime({ snapcast_host: v.trim() || null })) editingHost = false;
    }}
    onclose={() => (editingHost = false)}
  />
{/if}
{#if !thing && browsers.length}
  <Group title="Browsers" more="A page that plays in a browser: a player while it is open, never in a group.">
    {#each browsers as p (p.name)}{@render row(p)}{/each}
  </Group>
{/if}

{#if adjusting}
  {@const name = adjusting}
  {@const ms = delay(name)}
  <MenuSheet label="Speaker delay" title={name} chips={['Speaker delay']} onclose={() => (adjusting = null)}>
    <div class="delay">
      <button aria-label="Less" title="Less" disabled={ms <= 0} onclick={() => set(name, ms - STEP)}><Icon name="minus" size={20} /></button>
      <input type="range" use:panSafe min="0" max={maxOf(name)} step={STEP} value={ms} aria-label="Speaker delay of {name}" oninput={(e) => set(name, Number(e.currentTarget.value))} />
      <button aria-label="More" title="More" disabled={ms >= maxOf(name)} onclick={() => set(name, ms + STEP)}><Icon name="plus" size={20} /></button>
      <output>{ms} ms</output>
    </div>
  </MenuSheet>
{/if}
{#if removing}
  {@const name = removing}
  <ConfirmSheet title={`Remove ${name}?`} label="Remove" onconfirm={() => remove(name)} onclose={() => (removing = null)} />
{/if}

<style>
  .delay {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 56px;
    padding: 0 16px 16px;
  }

  .delay input {
    flex: 1;
    min-width: 0;
    accent-color: var(--art-accent, var(--accent));
  }

  .delay button {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--art-accent, var(--accent));
    cursor: pointer;
  }

  .delay button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .delay button:hover:not(:disabled),
  .delay button:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  output {
    flex: none;
    width: 6ch;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
