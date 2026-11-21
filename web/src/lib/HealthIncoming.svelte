<script lang="ts">
  // An Incoming album waiting, Health's third level (the owner, 2026-10-24): its intake card (lib/IntakeCard: checks on top,
  // tracks under), and its record's one sheet opened from Health › Incoming (lib/sheet/Record.svelte, RS01), where Add to,
  // Replace and Delete are as everywhere, and its copies side by side as its Versions (the copy waiting and the one it stands
  // against: SB23's one piece; SCENARIOS-DC1 OBJ3, SCENARIOS-HS01 IS.3); a failed conversion is tried again there.
  import { offers } from './capabilities.svelte';
  import RecordMenu from './sheet/Record.svelte';
  import { library, readLibrary } from './albums.svelte';
  import Icon from './Icon.svelte';
  import { loadHealth } from './healthReport.svelte';

  import { openRecordPlan } from './fileRuns.svelte';
  import IntakeCard from './IntakeCard.svelte';

  let { source, album, ongone }: { source: string; album: string; ongone: () => void } = $props();

  $effect(() => {
    if (!library.view) readLibrary(false).catch(() => {});
  });
  const s = $derived(library.view?.sources.find((x) => x.id === source) ?? null);
  const a = $derived(s?.albums.find((x) => x.id === album) ?? null);
  let menu = $state(false);
</script>

{#if !library.view}
  <p class="muted">…</p>
{:else if !s || !a}
  <p class="muted">No longer in the library.</p>
{:else}
  <!-- Cratarr's own: its intake card, checks on top, tracks under (lib/IntakeCard). -->
  <IntakeCard {source} {album} />
  <!-- What its tags would get before it comes in: reviewed on the sheet, written as a run (decision 0092, 0.14). -->
  {#if offers('intake_check')}<button class="act" onclick={() => openRecordPlan('intake', album)}>Before adding<Icon name="chevronRight" size={18} /></button>{/if}
  <button class="act" onclick={() => (menu = true)}>Add, replace or delete<Icon name="chevronRight" size={18} /></button>
  {#if menu}
    <RecordMenu
      id={a.id}
      from="incoming"
      title={a.title}
      credit={a.artist}
      onclose={() => (menu = false)}
      ongone={() => {
        readLibrary(true).catch(() => {});
        loadHealth();
        ongone();
      }}
    />
  {/if}
{/if}

<style>
  p {
    margin: 0;
  }

  .muted {
    color: var(--muted);
  }

  .act {
    justify-self: start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px 10px 18px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
</style>
