<script lang="ts">
  // Health (decision 0063, after 0025): every domain's findings from the health report, by domain, worst first, each
  // opening where it is dealt with; the Files domain with the files themselves, by kind, as folded sections of file
  // rows (lib/Section.svelte, lib/FileItem.svelte). The report is an administrator's; anyone else sees the files.
  import { getFileProblems, getHealthReport, type FileProblem, type HealthFinding } from './lib/api';
  import { DOMAINS, iconOf, openFinding } from './lib/health';
  import Icon from './lib/Icon.svelte';
  import FileItem from './lib/FileItem.svelte';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import Section from './lib/Section.svelte';

  let problems = $state<FileProblem[] | null>(null);
  let error = $state<string | null>(null);
  let findings = $state<HealthFinding[]>([]);
  $effect(() => {
    getHealthReport().then(
      (r) => (findings = r.findings),
      () => {},
    );
  });
  /** The domains with findings, in their order, each with its findings (the report's order: worst first). The files'
   *  own sections show their kinds, so their findings are not listed again. */
  const domains = $derived(
    Object.keys(DOMAINS)
      .map((d) => ({ id: d, ...DOMAINS[d], list: findings.filter((f) => f.domain === d) }))
      .filter((d) => d.list.length && d.id !== 'files'),
  );
  $effect(() => {
    getFileProblems().then(
      (p) => (problems = p),
      (e) => (error = e instanceof Error ? e.message : 'Unexpected error'),
    );
  });

  const KINDS: { kind: FileProblem['kind']; title: string; hint: string }[] = [
    { kind: 'broken', title: "Files that can't be read", hint: 'Not readable as audio at all. Usually a damaged or incomplete file; replace it.' },
    {
      kind: 'damaged_tag',
      title: 'Files with a damaged tag',
      hint: 'One tag field is broken and was skipped; the music and the other tags are fine. Saving the tags again in a tag editor fixes it.',
    },
    { kind: 'no_tags', title: 'Files without tags', hint: 'No artist, title or other tags at all.' },
    { kind: 'no_length', title: 'Files without a length', hint: "The audio's length could not be read." },
  ];
  const ofKind = (k: FileProblem['kind']) => (problems ?? []).filter((p) => p.kind === k);
</script>

{#snippet words(f: HealthFinding)}
  <span><b>{f.title}</b>{#if f.detail}<small>{f.detail}</small>{/if}</span>
{/snippet}

<main>
  {#if error}
    <Notice text={error} />
  {:else if !problems}
    <PageLoading />
  {:else if !problems.length && !domains.length}
    <p class="empty">Nothing needs you.</p>
  {:else}
    {#each domains as d (d.id)}
      <Section title={d.name}>
        <ul class="findings">
          {#each d.list as f (f.key)}
            <li>
              {#if f.open}
                <button class="finding" class:urgent={f.level === 'needs_you'} onclick={() => openFinding(f)}>
                  <Icon name={iconOf(f)} size={22} />{@render words(f)}<Icon name="chevronRight" size={20} />
                </button>
              {:else}
                <div class="finding" class:urgent={f.level === 'needs_you'}><Icon name={iconOf(f)} size={22} />{@render words(f)}</div>
              {/if}
            </li>
          {/each}
        </ul>
      </Section>
    {/each}
    {#each KINDS as k (k.kind)}
      {@const list = ofKind(k.kind)}
      {#if list.length > 0}
        <Section title={k.title} count={`${list.length} ${list.length === 1 ? 'file' : 'files'}`} hint={k.hint} open={false}>
          <ul class="rows">
            {#each list as p (p.folder + p.rel)}
              <li><FileItem path={p.rel} detail={p.detail ?? undefined} /></li>
            {/each}
          </ul>
        </Section>
      {/if}
    {/each}
  {/if}
</main>

<style>
  main {
    max-width: 900px;
    padding: 16px clamp(12px, 2vw, 32px) 48px;
  }

  .empty {
    margin-top: 32px;
    color: var(--muted);
    font-size: 0.9rem;
  }

  .rows,
  .findings {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* A finding: its domain's icon (red when the owner has to act), its words and their detail, where it opens. */
  .finding {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    min-height: 52px;
    padding: 6px 4px;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
  }

  button.finding {
    cursor: pointer;
  }

  .finding :global(svg:first-child) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  .finding.urgent :global(svg:first-child) {
    color: var(--danger);
  }

  .finding :global(svg:last-child:not(:first-child)) {
    flex: none;
    color: var(--muted);
  }

  .finding span {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
  }

  .finding b {
    font-weight: 500;
  }

  .finding small {
    overflow: hidden;
    color: var(--muted);
    font-size: 0.85rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
