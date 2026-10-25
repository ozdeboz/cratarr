<script lang="ts">
  // Requests (decision 0070; laid out as Downloads, Insights, Health and Settings, the owner 2026-10-24): an index of
  // its lists, each its icon, its name, how many in a line, and ›. A tap opens the list in the panel (lib/RequestsPanel):
  // beside the index on a roomy window, where one is always open (App), its row tinted; over it on a phone. Waiting for
  // an answer only for those who answer requests.
  import { panelBeside } from './lib/glass.svelte';
  import Icon from './lib/Icon.svelte';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { panel, REQUESTS, requestsOf, viewPlaylist } from './lib/panel.svelte';
  import { answersRequests, loadRequests, lookAtLidarr, REQUESTS_LISTS, requests, type RequestsList } from './lib/requests.svelte';

  loadRequests();
  $effect(() => {
    lookAtLidarr();
    const t = setInterval(lookAtLidarr, 30_000);
    return () => clearInterval(t);
  });

  const lists = $derived(REQUESTS_LISTS.filter((l) => l.id !== 'waiting' || answersRequests()));
  const count = (n: number, one: string, many: string) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
  /** A list's line: how many, and how they stand. */
  function line(id: RequestsList): string | null {
    const v = requests.view;
    if (!v) return null;
    if (id === 'waiting') return v.waiting?.length ? count(v.waiting.length, 'request waits', 'requests wait') : 'None waiting';
    if (id === 'mine') {
      if (!v.mine.length) return 'None yet';
      const pending = v.mine.filter((r) => r.status === 'pending').length;
      return [count(v.mine.length, 'request', 'requests'), pending ? `${pending} waiting` : null].filter(Boolean).join(' · ');
    }
    const w = requests.watching;
    if (!v.fetcher) return 'Not connected';
    if (!w) return null;
    return [w.fetching.length ? `${w.fetching.length} fetching` : null, count(w.looking.length + w.more, 'not found yet', 'not found yet')].filter(Boolean).join(' · ');
  }
</script>

<main>
  {#if requests.error && !requests.view}
    <Notice text={requests.error} />
  {:else if !requests.view}
    <PageLoading />
  {:else}
    <section class="group" aria-label="Requests">
      <div class="list">
        {#each lists as l (l.id)}
          {@const open = panelBeside() && requestsOf(panel.viewed) === l.id}
          {@const said = line(l.id)}
          <button class="row" class:open aria-pressed={open} onclick={() => viewPlaylist(REQUESTS + l.id)}>
            <Icon name={l.icon} size={24} />
            <span class="text">
              <span class="name">{l.label}</span>
              {#if said}<small>{said}</small>{/if}
            </span>
            <span class="chev"><Icon name="chevronRight" size={20} /></span>
          </button>
        {/each}
      </div>
    </section>
  {/if}
</main>

<style>
  /* As Insights' list (Insights.svelte): one column, beside a list it is the index's own column (App). */
  main {
    padding: 16px clamp(12px, 2vw, 32px) 96px;
  }

  .group {
    max-width: 640px;
  }

  .list {
    overflow: hidden;
    border-radius: 14px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 64px;
    padding: 10px 14px 10px 16px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .row:hover,
  .row:focus-visible {
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  /* The list open beside the index. */
  .row.open {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .row > :global(svg) {
    flex: none;
    color: var(--accent);
  }

  .text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .name {
    font-size: 15px;
  }

  .text small {
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chev {
    display: grid;
    flex: none;
    color: var(--muted);
  }
</style>
