<script lang="ts">
  // Home's System status (decision 0063; named so by the owner, 2026-10-23): what matters most of the health report,
  // the most critical first and the most recent among equals (the owner: "the most important, the most critical, the
  // most recent"), a few,
  // each opening Health on it, in its panel (the owner, 2026-10-23); its heading's arrow the Health page, where all of it is. What needs the owner
  // and what is wrong; of what is worth knowing only AudioMuse's progress (the owner's tile). Nothing: it does not show.
  // An administrator's only: the report is theirs.
  import { getHealthReport, type HealthFinding } from './api';
  import { iconOf, openInHealth } from './health';
  import Icon from './Icon.svelte';
  import { me } from './me.svelte';
  import SectionHead from './SectionHead.svelte';

  /** At most this many rows. */
  const MOST = 4;

  /** `onsettled`: once its answer is in (or there is none to ask for), so Home can show it in its turn. */
  let { onsettled }: { onsettled?: () => void } = $props();

  let findings = $state<HealthFinding[]>([]);

  $effect(() => {
    if (!me.admin) {
      if (me.known) onsettled?.();
      return;
    }
    let gone = false;
    getHealthReport().then(
      (r) => {
        if (gone) return;
        findings = r.findings;
        onsettled?.();
      },
      () => !gone && onsettled?.(),
    );
    return () => (gone = true);
  });

  // The report comes worst first, the most recent first among equals.
  const rows = $derived(findings.filter((f) => f.level !== 'info' || f.of !== undefined).slice(0, MOST));
</script>

{#if me.admin && rows.length}
  <section aria-label="System status">
    <!-- Its arrow: the Health page, where all of it is (decision 0063). -->
    <SectionHead title="System status" onmore={() => (location.hash = '#/health')} />
    <div class="needs">
      {#each rows as f (f.key)}
        {#if f.open}
          <button class="row" class:urgent={f.level === 'critical'} onclick={() => openInHealth(f)}
            ><Icon name={iconOf(f)} size={22} /><span>{f.title}</span><Icon name="chevronRight" size={20} /></button
          >
        {:else}
          <div class="row" class:urgent={f.level === 'critical'}>
            <Icon name={iconOf(f)} size={22} /><span
              >{f.title}{#if f.of}<i class="bar" style:--share={f.count / f.of} aria-hidden="true"></i>{/if}</span
            >
          </div>
        {/if}
      {/each}
    </div>
  </section>
{/if}

<style>
  /* As Home's other lists: 16 px in, 8 px apart. */
  .needs {
    display: grid;
    gap: 8px;
    margin: 4px 16px 0;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 52px;
    padding: 0 14px;
    border: 0;
    border-radius: 12px;
    /* Grey, as Home's other rows (Jump back in, Blast from the past): the colour is the icon's alone (the owner,
       2026-10-24: the green fill broke the grey). */
    background: color-mix(in srgb, var(--text) 6%, transparent);
    color: var(--text);
    font: inherit;
    font-size: 16px;
    text-align: left;
  }

  button.row {
    cursor: pointer;
  }

  .row :global(svg:first-child) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  /* The owner has to act: its icon says so. */
  .row.urgent :global(svg:first-child) {
    color: var(--danger);
  }

  .row :global(svg:last-child:not(:first-child)) {
    flex: none;
    color: var(--muted);
  }

  .row span {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 6px;
  }

  /* A share (AudioMuse's tracks heard), as a thin line under its words. */
  .bar {
    display: block;
    height: 3px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--art-accent, var(--accent)) calc(var(--share) * 100%),
      color-mix(in srgb, var(--text) 15%, transparent) calc(var(--share) * 100%)
    );
  }
</style>
