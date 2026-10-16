<script lang="ts">
  // Settings → Players (sync play, decision 0037): each player's speaker delay, how late its speaker sounds, so a group
  // sounds as one. Set for any player from any device (standing between two speakers), heard at once while the finger
  // moves, as Snapweb and Iris set Snapcast's per-client latency. 0 to 500 ms in steps of 5.
  import { setPlayerDelay } from './api';
  import Icon from './Icon.svelte';
  import { playOn } from './playOn.svelte';
  import { toast } from './toast.svelte';

  const MAX = 500;
  const STEP = 5;

  const players = $derived([...playOn.players].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase())));

  // While it moves, its value here; the player list takes over after.
  let held = $state<Record<string, number>>({});
  const delay = (name: string) => held[name] ?? playOn.players.find((p) => p.name === name)?.delay_ms ?? 0;

  const busy = new Map<string, ReturnType<typeof setTimeout>>();
  const wanted = new Map<string, number>();
  const letGo = new Map<string, ReturnType<typeof setTimeout>>();

  /** At most one save per player every 100 ms while it moves; the last always goes. */
  function set(name: string, ms: number) {
    const v = Math.min(MAX, Math.max(0, Math.round(ms / STEP) * STEP));
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
</script>

<section aria-label="Players">
  {#each players as p (p.name)}
    {@const ms = delay(p.name)}
    <div class="player">
      <span class="name">{p.name}{#if !p.online}<small>offline</small>{/if}</span>
      <span class="l">Speaker delay <small>How late it sounds</small></span>
      <div class="delay">
        <button aria-label="Less" title="Less" disabled={ms <= 0} onclick={() => set(p.name, ms - STEP)}><Icon name="minus" size={20} /></button>
        <input
          type="range"
          min="0"
          max={MAX}
          step={STEP}
          value={ms}
          aria-label="Speaker delay of {p.name}"
          oninput={(e) => set(p.name, Number(e.currentTarget.value))}
        />
        <button aria-label="More" title="More" disabled={ms >= MAX} onclick={() => set(p.name, ms + STEP)}><Icon name="plus" size={20} /></button>
        <output>{ms} ms</output>
      </div>
    </div>
  {:else}
    <p class="none">No players yet</p>
  {/each}
</section>

<style>
  section {
    display: grid;
    gap: 12px;
    padding: 4px 0 16px;
  }

  .player {
    display: grid;
    gap: 4px;
    padding: 12px 16px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  .name {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 16px;
    font-weight: 600;
  }

  small {
    color: var(--muted);
    font-size: 13px;
    font-weight: 400;
  }

  .l {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 15px;
  }

  .delay {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 44px;
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

  .none {
    color: var(--muted);
  }
</style>
