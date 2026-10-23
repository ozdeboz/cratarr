<script lang="ts">
  // Settings → Players (sync play, decision 0037): each player's speaker delay, how late its speaker sounds, so a group
  // sounds as one. Set for any player from any device (standing between two speakers), heard at once while the finger
  // moves, as Snapweb and Iris set Snapcast's per-client latency. 0 to 500 ms in steps of 5. What every player has is
  // said once (the group's heading), each player's value on its row; a tap opens its slider in a sheet (the settings'
  // North Star, 2026-10-19). Then the Kodis (lib/KodiSettings), each with its own page (`thing`).
  import { setPlayerDelay } from './api';
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

  const players = $derived([...playOn.players].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase())));

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

  // The player whose delay is being set, in its sheet.
  let adjusting = $state<string | null>(null);
</script>

{#if !thing}
  <Group title="Speaker delay" more={DELAY_MORE}>
    {#each players as p (p.name)}
      <Item lead={p.online ? 'on' : 'off'} name={p.name} line={p.online ? null : 'Offline'} value={`${delay(p.name)} ms`} onclick={() => (adjusting = p.name)} />
    {:else}
      {#if playOn.known}<Item name="No players yet" />{/if}
    {/each}
  </Group>
{/if}
<KodiSettings {thing} />

{#if adjusting}
  {@const name = adjusting}
  {@const ms = delay(name)}
  <MenuSheet label="Speaker delay" title={name} chips={['Speaker delay']} onclose={() => (adjusting = null)}>
    <div class="delay">
      <button aria-label="Less" title="Less" disabled={ms <= 0} onclick={() => set(name, ms - STEP)}><Icon name="minus" size={20} /></button>
      <input type="range" min="0" max={maxOf(name)} step={STEP} value={ms} aria-label="Speaker delay of {name}" oninput={(e) => set(name, Number(e.currentTarget.value))} />
      <button aria-label="More" title="More" disabled={ms >= maxOf(name)} onclick={() => set(name, ms + STEP)}><Icon name="plus" size={20} /></button>
      <output>{ms} ms</output>
    </div>
  </MenuSheet>
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
