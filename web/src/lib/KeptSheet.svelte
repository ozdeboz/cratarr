<script lang="ts">
  // The progress sheet of what is kept on this device (decision 0038), opened from the Downloads page's ring: each kept
  // item and how far it is, the track coming in now, the storage against the budget. A kept item can be let go here;
  // failed tracks are tried again with one tap.
  import Icon from './Icon.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Meter from './Meter.svelte';
  import { kept, setKept, syncKept, type KeptItem } from './kept.svelte';
  import { downloadedSize } from './settings/editor.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const WAITING = { wifi: 'Waiting for Wi-Fi', charging: 'Waiting to charge', network: 'Waiting for a connection' } as const;

  /** What an item's line says under its name. */
  function line(i: KeptItem): string {
    if (i.state === 'error') return i.error ?? 'Cannot be read';
    const want = i.total - i.over_budget;
    const parts = [`${i.done} of ${want}`];
    if (i.over_budget) parts.push(`${i.over_budget} over budget`);
    if (i.failed) parts.push(`${i.failed} failed`);
    if (i.state === 'waiting' && kept.waiting) parts.push(WAITING[kept.waiting]);
    if (i.bytes) parts.push(downloadedSize(i.bytes));
    return parts.join(' · ');
  }
  const failed = $derived(kept.items.some((i) => i.failed > 0));
</script>

<MenuSheet label="Sync" {onclose} images={null} icon="arrowDownToLine" title="Sync" chips={kept.waiting ? [WAITING[kept.waiting]] : []}>
  <ul class="items">
    {#each kept.items as i (i.key)}
      {@const want = i.total - i.over_budget}
      <li>
        <div class="text">
          <strong>{i.name}</strong>
          {#if i.state === 'syncing' && want > 0}
            <Meter label={line(i)} value={i.done / want} />
          {:else}
            <small class:bad={i.state === 'error' || i.failed > 0}>{line(i)}</small>
          {/if}
        </div>
        <button class="drop" aria-label={`Stop keeping ${i.name}`} title="Stop keeping" onclick={() => setKept(i.kind, i.source, i.id, false)}
          ><Icon name="close" size={20} /></button
        >
      </li>
    {:else}
      <li class="none">Nothing kept.</li>
    {/each}
  </ul>

  {#if kept.current}<p class="now">{kept.current}</p>{/if}
  <div class="foot">
    {#if kept.budget}
      <Meter label={`${downloadedSize(kept.used)} of ${downloadedSize(kept.budget)}`} value={kept.used / kept.budget} />
    {:else}
      <small>{downloadedSize(kept.used)}</small>
    {/if}
    {#if failed}<button class="retry" onclick={syncKept}>Try again</button>{/if}
  </div>
</MenuSheet>

<style>
  .items {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0 16px;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 56px;
  }

  .text {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  strong {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  small {
    color: var(--muted);
    font-size: 0.8rem;
  }

  small.bad {
    color: var(--danger-ink);
  }

  .none {
    color: var(--muted);
  }

  .drop {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .drop:hover {
    background: var(--card-hover);
    color: var(--text);
  }

  .now {
    margin: 8px 16px 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 0.85rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 16px 8px;
  }

  .retry {
    min-height: 40px;
    padding: 0 16px;
    border: 1px solid var(--border);
    border-radius: 20px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
</style>
