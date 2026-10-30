<script lang="ts">
  // The Library's Classical, first (0.10; the owner, 2026-10-30: "it opens on a composer list"): its composers, by their
  // sort name, each with how many records, as the Library's artists were listed (a round picture where one is known); a
  // composer opens their records (lib/classical). The page's search narrows them.
  import Cover from './Cover.svelte';
  import PageLoading from './PageLoading.svelte';
  import { matchesSearch } from './pageView.svelte';
  import Row from './Row.svelte';
  import { classical, loadComposers, openComposer } from './classical.svelte';
  import { plural } from './insights.svelte';

  $effect(() => loadComposers());
  const shown = $derived((classical.composers ?? []).filter((c) => matchesSearch(c.name, c.sort_name)));
</script>

{#if classical.error && !classical.composers}
  <p class="why" role="alert">{classical.error}</p>
{:else if !classical.composers}
  <PageLoading />
{:else if !shown.length}
  <p class="empty">{classical.composers.length ? 'No composers match.' : 'No classical records yet.'}</p>
{:else}
  <ul class="rows" aria-label="Composers">
    {#each shown as c (c.key)}
      <li>
        <Row size="large" title={c.name} sub={plural(c.records, 'record')} label={`Open ${c.name}`} onclick={() => openComposer(c)}>
          {#snippet front()}<span class="art"><Cover src={c.photo ?? null} /></span>{/snippet}
        </Row>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .rows {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .art {
    position: relative;
    display: block;
    width: 48px;
    height: 48px;
    overflow: hidden;
    border-radius: 50%;
  }

  .empty,
  .why {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }
</style>
