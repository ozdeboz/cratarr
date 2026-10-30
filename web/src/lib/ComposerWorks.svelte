<script lang="ts">
  // A composer's works (0.12, decision 0086; the owner, 2026-10-30: "works first, records below"): over their records,
  // the most recorded first, each with how many recordings the library holds (and its catalogue number and key); a tap
  // opens the work (lib/WorkSheet). The first few, the rest a tap away. Only when the server says `works`; a composer
  // with none shows nothing here, the records as before.
  import { getComposerWorks } from './api';
  import Row from './Row.svelte';
  import WorkSheet from './WorkSheet.svelte';
  import { workLine, worksInOrder, type WorkListed } from './works';

  let { composer }: { composer: string } = $props();

  /** How many show before "Show all". */
  const FIRST = 6;
  let works = $state<WorkListed[] | null>(null);
  let all = $state(false);
  let open = $state<WorkListed | null>(null);
  $effect(() => {
    const key = composer;
    let gone = false;
    works = null;
    all = false;
    getComposerWorks(key).then(
      (l) => !gone && (works = worksInOrder(l ?? [])),
      () => !gone && (works = []),
    );
    return () => (gone = true);
  });
  const shown = $derived(works ? (all ? works : works.slice(0, FIRST)) : []);
</script>

{#if works?.length}
  <section class="works" aria-label="Works">
    <h3>Works</h3>
    <ul class="rows">
      {#each shown as w (w.work)}
        <li><Row size="medium" title={w.title} sub={workLine(w)} label="Open {w.title}" onclick={() => (open = w)} /></li>
      {/each}
      {#if works.length > FIRST && !all}
        <li><Row size="medium" title="Show all {works.length} works" onclick={() => (all = true)} /></li>
      {/if}
    </ul>
    <h3>Records</h3>
  </section>
{/if}

{#if open}
  {@const w = open}
  <WorkSheet mbid={w.work} title={w.title} onclose={() => (open = null)} />
{/if}

<style>
  .works h3 {
    margin: 12px 0 6px;
    color: var(--muted);
    font-size: 0.85rem;
    font-weight: 600;
  }

  .rows {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
