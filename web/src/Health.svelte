<script lang="ts">
  // Health (decision 0063), laid out as Settings (0.13; the owner, 2026-10-31, mockup C: "keeping a straight style"):
  // three areas (the owner, 2026-11-01): Library, Connections, Activity (lib/health AREAS), each under its small
  // heading, its problems most urgent first (lib/healthThrough byArea); only problems show. Each a row
  // (lib/settings/Item): a dot (red: it needs you, amber: a warning, grey: to know), the thing it is about, one line,
  // how many and how it stands ("Covers · 3 missing", where the server says so), and ›. A tap opens the problem in the panel
  // (lib/HealthPanel, one layout for every problem): beside the list on a roomy window, where one is always open (App:
  // the one last open, else the first), its row tinted; over the list on a phone. The report is an administrator's;
  // anyone else sees the files.
  import { panelBeside } from './lib/glass.svelte';
  import { health, loadHealth, problems } from './lib/healthReport.svelte';
  import { byArea, dotOf } from './lib/healthThrough';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { HEALTH, healthOf, healthThing, panel, viewPlaylist } from './lib/panel.svelte';
  import { matchesSearch, pageView, searchNone } from './lib/pageView.svelte';
  import Group from './lib/settings/Group.svelte';
  import Item from './lib/settings/Item.svelte';
  import { fileFacts, loadFileFacts } from './lib/fileFacts.svelte';
  import { CHANGES, fileRuns, loadRuns, openChanges, openReview } from './lib/fileRuns.svelte';
  import { AREAS } from './lib/health';
  import { me } from './lib/me.svelte';

  $effect(loadHealth);
  // The pill's search (the owner, 2026-10-25): a problem whose words, or one of its items', hold every word.
  const list = $derived(problems().filter((p) => matchesSearch(p.domain, p.title, p.line, ...p.items.map((x) => `${x.title} ${x.line ?? ''}`))));
  // Its words leave none: the pill says so, from its magnifier (pageView searchNone).
  searchNone(() => health.loaded && !list.length && { n: problems().length, one: 'problem' });
  const groups = $derived(byArea(list, AREAS));
  // Changes to files (decision 0092; administrators'): the records whose files differ from their types and years (while
  // types into files is on) a line under Library, opening its review; every run one line under Files, naming what
  // writes and waits, with the meter its panel shows at its top (the owner, 2026-11-04: its progress under the row's
  // title too); no Undo of its own.
  $effect(() => loadFileFacts());
  $effect(() => loadRuns());
  const differ = $derived(fileFacts.value?.on ? fileFacts.value.differ : 0);
  const now = $derived((fileRuns.list ?? []).filter((r) => r.state !== 'done'));
  const changesLine = $derived.by(() => {
    const writing = now.find((r) => r.state === 'writing' || r.state === 'adding');
    const waiting = now.filter((r) => r.state === 'waiting').length;
    const parts = [writing ? `${writing.title} ${writing.state === 'adding' ? 'adding' : 'writing'}` : null, waiting ? `${waiting} waiting` : null].filter(Boolean);
    return parts.length ? parts.join(' · ') : null;
  });
  /** The run under way's meter, as the panel shows it: how much of it is written, nothing yet while it waits. */
  const changesProgress = $derived.by(() => {
    const writing = now.find((r) => r.state === 'writing');
    if (writing?.of) return writing.done / writing.of;
    return now.some((r) => r.state === 'waiting') ? 0 : null;
  });
  // Activity shows only while something runs or ran this month (the owner, 2026-11-01: only what matters shows).
  const showChanges = $derived(me.admin && !!fileRuns.list?.length);
  const opened = (thing: string | null) => panelBeside() && healthOf(panel.viewed) === CHANGES && healthThing(panel.viewed) === thing;
  const facts = $derived(differ > 0 || showChanges);
  // A run ended: how many differ is counted again.
  const ended = $derived((fileRuns.list ?? []).filter((r) => r.state === 'done').length);
  $effect(() => {
    void ended;
    loadFileFacts();
  });
  const LIBRARY = 'Library';
  const ACTIVITY = 'Activity';
  /** Whether a row's line says how many already ("98 albums without a genre"): its value then says nothing more (the
   *  owner, 2026-11-03: the same number twice per row is wrong). */
  const saysCount = (p: ReturnType<typeof problems>[number], line: string | null) => {
    const count = p.finding?.count ?? p.items.length;
    return !!line && line.trim().split(/\s/)[0] === count.toLocaleString();
  };
  /** How many, and how it stands where the server says ("3 missing"): the finding's count, else its items. */
  const countOf = (p: ReturnType<typeof problems>[number]) => {
    const count = p.finding?.count ?? p.items.length;
    const state = p.finding?.state;
    // A state with nothing to count says only itself ("Salon · silent").
    if (state) return count ? `${count.toLocaleString()} ${state}` : state;
    return count.toLocaleString();
  };
</script>

<main>
  <!-- Either answer failed: said in place, never "All good." (the owner, 2026-11-02: errors are never shown as nothing). -->
  {#if health.reportError}<Notice text={health.reportError} onretry={loadHealth} />{/if}
  {#if health.error && health.error !== health.reportError}<Notice text={health.error} onretry={loadHealth} />{/if}
  {#if !health.loaded}
    <PageLoading />
  {:else if !list.length && !facts}
    {#if !problems().length}{#if !health.reportError && !health.error}<p class="empty">All good.</p>{/if}{:else if !pageView.none}<p class="empty">None of them match.</p>{/if}
  {:else}
    <div class="groups">
      {#each groups as g (g.area)}
        <Group title={g.area}>
          {#each g.problems as p (p.key)}
            {@const line = p.name === p.title ? p.line : p.title}
            <Item
              lead={dotOf(p)}
              name={p.name}
              {line}
              value={saysCount(p, line) ? null : countOf(p)}
              open={panelBeside() && healthOf(panel.viewed) === p.key}
              onclick={() => viewPlaylist(HEALTH + p.key)}
            />
          {/each}
          {#if g.area === LIBRARY}{@render differItem()}{/if}
          {#if g.area === ACTIVITY}{@render changesItem()}{/if}
        </Group>
      {/each}
      {#if differ > 0 && !groups.some((g) => g.area === LIBRARY)}<Group title={LIBRARY}>{@render differItem()}</Group>{/if}
      {#if showChanges && !groups.some((g) => g.area === ACTIVITY)}<Group title={ACTIVITY}>{@render changesItem()}</Group>{/if}
    </div>
  {/if}
</main>

{#snippet differItem()}
  {#if differ > 0}<Item lead="off" name="Files that differ from their types and years" line="Review before writing" value={differ.toLocaleString()} open={opened('types')} onclick={() => openReview('types')} />{/if}
{/snippet}
{#snippet changesItem()}
  {#if showChanges}<Item lead="off" name="Changes to files" line={changesLine} progress={changesProgress} open={opened(null)} onclick={openChanges} />{/if}
{/snippet}

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
