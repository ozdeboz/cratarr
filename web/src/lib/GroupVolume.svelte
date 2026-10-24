<script lang="ts">
  // Sync play's volumes (decision 0037, as Snapweb's and Music Assistant's): each grouped device's own, and the group's,
  // the devices' average. Moving the group's moves each device in proportion, from where they were when the finger went
  // down: up toward 100, down toward 0. A device's volume stays its own (Play On's `volume`): in the app Android's media
  // volume, in a browser the page's. One device alone (a page in a browser, whose volume nothing else on the device
  // shows): its row only.
  import { panSafe } from './panSafe';
  import { playerCommand } from './api';
  import { playOn, selfName } from './playOn.svelte';

  let { names }: { names: string[] } = $props();

  // While a finger moves a slider, its values here; the players' reports take over after.
  let held = $state<Record<string, number>>({});
  let from: Record<string, number> | null = null;
  const reported = (n: string) => playOn.players.find((p) => p.name === n)?.volume ?? 100;
  const vol = (n: string) => held[n] ?? reported(n);
  const group = $derived(Math.round(names.reduce((a, n) => a + vol(n), 0) / Math.max(names.length, 1)));
  const shown = (n: string) => (n === selfName() ? 'This device' : n);

  // At most a command per device every 150 ms while it moves; the last always goes.
  const busy = new Map<string, ReturnType<typeof setTimeout>>();
  const wanted = new Map<string, number>();
  function send(n: string, level: number) {
    held[n] = level;
    wanted.set(n, level);
    if (busy.has(n)) return;
    const go = () => {
      const l = wanted.get(n) ?? level;
      playerCommand(n, { command: 'volume', level: l }).catch(() => {});
      busy.set(
        n,
        setTimeout(() => {
          busy.delete(n);
          if (wanted.get(n) !== l) go();
        }, 150),
      );
    };
    go();
  }

  let letGo: ReturnType<typeof setTimeout> | undefined;
  function done() {
    from = null;
    clearTimeout(letGo);
    letGo = setTimeout(() => (held = {}), 2000);
  }

  function setGroup(g: number) {
    from ??= Object.fromEntries(names.map((n) => [n, vol(n)]));
    const was = from;
    const avg = names.reduce((a, n) => a + was[n], 0) / names.length;
    for (const n of names) {
      const v = was[n];
      const next = g >= avg ? v + ((100 - v) * (g - avg)) / Math.max(100 - avg, 1) : (v * g) / Math.max(avg, 1);
      send(n, Math.round(Math.min(100, Math.max(0, next))));
    }
  }
</script>

<div class="volumes">
  {#if names.length > 1}
    <label class="row group">
      <span>Group</span>
      <input type="range" use:panSafe min="0" max="100" step="1" value={group} oninput={(e) => setGroup(Number(e.currentTarget.value))} onchange={done} />
    </label>
  {/if}
  {#each names as n (n)}
    <label class="row">
      <span>{shown(n)}</span>
      <input type="range" use:panSafe min="0" max="100" step="1" value={vol(n)} oninput={(e) => send(n, Number(e.currentTarget.value))} onchange={done} />
    </label>
  {/each}
</div>

<style>
  .volumes {
    display: grid;
    gap: 2px;
    margin: 8px 12px 4px;
    padding: 6px 14px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    font-size: 15px;
  }

  .row span {
    flex: none;
    width: 34%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row.group {
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }

  .row input {
    flex: 1;
    min-width: 0;
    accent-color: var(--art-accent, var(--accent));
  }
</style>
