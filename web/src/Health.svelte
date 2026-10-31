<script lang="ts">
  // Health (decision 0063), laid out as Settings (0.13; the owner, 2026-10-31, mockup C: "keeping a straight style"):
  // the problems grouped by their domain, each group under its small heading, a domain where its worst problem stands
  // and its problems worst first (lib/healthThrough byDomain); each a row (lib/settings/Item): a dot (red: it needs you,
  // amber: a warning, grey: to know), its name, one line, how many, and ›. A tap opens the problem in the panel
  // (lib/HealthPanel, one layout for every problem): beside the list on a roomy window, where one is always open (App:
  // the one last open, else the first), its row tinted; over the list on a phone. The report is an administrator's;
  // anyone else sees the files.
  import { panelBeside } from './lib/glass.svelte';
  import { health, loadHealth, problems } from './lib/healthReport.svelte';
  import { byDomain, dotOf } from './lib/healthThrough';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { HEALTH, healthOf, panel, viewPlaylist } from './lib/panel.svelte';
  import { matchesSearch } from './lib/pageView.svelte';
  import Group from './lib/settings/Group.svelte';
  import Item from './lib/settings/Item.svelte';

  $effect(loadHealth);
  // The pill's search (the owner, 2026-10-25): a problem whose words, or one of its items', hold every word.
  const list = $derived(problems().filter((p) => matchesSearch(p.domain, p.title, p.line, ...p.items.map((x) => `${x.title} ${x.line ?? ''}`))));
  const groups = $derived(byDomain(list));
  /** How many: the finding's count, else its items. */
  const countOf = (p: ReturnType<typeof problems>[number]) => (p.finding?.count ?? p.items.length).toLocaleString();
</script>

<main>
  {#if health.error}<Notice text={health.error} />{/if}
  {#if !health.loaded}
    <PageLoading />
  {:else if !list.length}
    <p class="empty">{problems().length ? 'None of them match.' : 'All good.'}</p>
  {:else}
    <div class="groups">
      {#each groups as g (g.domain)}
        <Group title={g.domain}>
          {#each g.problems as p (p.key)}
            <Item
              lead={dotOf(p)}
              name={p.name}
              line={p.name === p.title ? p.line : p.title}
              value={countOf(p)}
              open={panelBeside() && healthOf(panel.viewed) === p.key}
              onclick={() => viewPlaylist(HEALTH + p.key)}
            />
          {/each}
        </Group>
      {/each}
    </div>
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

  .groups {
    max-width: 640px;
  }
</style>
