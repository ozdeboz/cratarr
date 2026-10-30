<script lang="ts">
  // Health (decision 0063; laid out as Settings, the owner 2026-10-23: "one card per problem"): a card per problem,
  // worst first: its domain's icon (red when it needs the owner), what it is, a line of detail, and ›. A tap opens the
  // problem in the panel (lib/HealthPanel, one layout for every problem): beside the list on a roomy window, where one is
  // always open (App: the one last open, else the first), its card tinted; over the list on a phone. The report is an
  // administrator's; anyone else sees the files.
  import { panelBeside } from './lib/glass.svelte';
  import Icon from './lib/Icon.svelte';
  import Meter from './lib/Meter.svelte';
  import { health, loadHealth, problems } from './lib/healthReport.svelte';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { HEALTH, healthOf, panel, viewPlaylist } from './lib/panel.svelte';
  import { matchesSearch } from './lib/pageView.svelte';

  $effect(loadHealth);
  // The pill's search (the owner, 2026-10-25): a problem whose words, or one of its items', hold every word.
  const list = $derived(problems().filter((p) => matchesSearch(p.domain, p.title, p.line, ...p.items.map((x) => `${x.title} ${x.line ?? ''}`))));
</script>

<main>
  {#if health.error}<Notice text={health.error} />{/if}
  {#if !health.loaded}
    <PageLoading />
  {:else if !list.length}
    <p class="empty">{problems().length ? 'None of them match.' : 'All good.'}</p>
  {:else}
    <ul class="cards">
      {#each list as p (p.key)}
        {@const open = panelBeside() && healthOf(panel.viewed) === p.key}
        <li>
          <button class="card" class:open class:urgent={p.urgent} aria-pressed={open} onclick={() => viewPlaylist(HEALTH + p.key)}>
            <Icon name={p.icon} size={24} />
            <span class="text">
              <small class="domain">{p.domain}</small>
              <span class="name">{p.title}</span>
              <!-- With a whole (how many of how many): the line over the bar, as a row with work under way (lib/settings/Item,
                   the owner 2026-10-25). -->
              {#if p.finding?.of}<Meter label={p.line ?? `${p.finding.count.toLocaleString()} of ${p.finding.of.toLocaleString()}`} value={p.finding.count / p.finding.of} />{:else if p.line}<small>{p.line}</small>{/if}
            </span>
            <span class="chev"><Icon name="chevronRight" size={20} /></span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</main>

<style>
  /* One column, as Settings' list: beside a problem it is the list's own column (App). */
  main {
    padding: 16px clamp(12px, 2vw, 32px) 96px;
  }

  .empty {
    margin-top: 32px;
    color: var(--muted);
  }

  .cards {
    display: grid;
    gap: 8px;
    max-width: 640px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .card {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 64px;
    padding: 12px 14px 12px 16px;
    border: 0;
    border-radius: 14px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .card:hover,
  .card:focus-visible {
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  /* The problem open beside the list. */
  .card.open {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .card > :global(svg) {
    flex: none;
    color: var(--accent);
  }

  .card.urgent > :global(svg) {
    color: var(--danger);
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

  .text .domain {
    font-size: 12px;
  }

  .chev {
    display: grid;
    flex: none;
    color: var(--muted);
  }
</style>
