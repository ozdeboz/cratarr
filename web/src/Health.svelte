<script lang="ts">
  // Health (decision 0063, after 0025; laid out as Settings, the owner 2026-10-23): a row per domain, its icon (red when
  // something needs the owner), its name, what it says now in a line, and ›. A tap opens the domain in the panel
  // (lib/HealthPanel): its findings, Files' files by kind; beside the list on a roomy window, where one is always open
  // (App: the one last open, else the first with something), its row tinted; over the list on a phone. The report is
  // an administrator's; anyone else sees the files.
  import Icon from './lib/Icon.svelte';
  import { DOMAINS } from './lib/health';
  import { FILE_KINDS, health, loadHealth } from './lib/healthReport.svelte';
  import Notice from './lib/Notice.svelte';
  import { HEALTH, healthOf, panel, viewPlaylist } from './lib/panel.svelte';

  $effect(loadHealth);

  /** What a domain says now, in a line, and whether it needs the owner. */
  function state(id: string): { line: string; urgent: boolean } {
    const list = health.findings.filter((f) => f.domain === id);
    if (id === 'files') {
      const n = (health.problems ?? []).filter((p) => FILE_KINDS.some((k) => k.kind === p.kind)).length;
      if (health.problems === null) return { line: '', urgent: false };
      return { line: n ? `${n} ${n === 1 ? 'file' : 'files'} to look at` : 'All good', urgent: list.some((f) => f.level === 'needs_you') };
    }
    if (!list.length) return { line: 'All good', urgent: false };
    return { line: list.length === 1 ? list[0].title : `${list.length} things to look at`, urgent: list.some((f) => f.level === 'needs_you') };
  }
</script>

<main>
  {#if health.error}<Notice text={health.error} />{/if}
  <section class="group" aria-label="Health">
    <div class="list">
      {#each Object.entries(DOMAINS) as [id, d] (id)}
        {@const open = healthOf(panel.viewed) === id}
        {@const st = state(id)}
        <button class="row" class:open class:urgent={st.urgent} aria-pressed={open} onclick={() => viewPlaylist(HEALTH + id)}>
          <Icon name={d.icon} size={24} />
          <span class="text">
            <span class="name">{d.name}</span>
            {#if st.line}<small>{st.line}</small>{/if}
          </span>
          <span class="chev"><Icon name="chevronRight" size={20} /></span>
        </button>
      {/each}
    </div>
  </section>
</main>

<style>
  /* As Settings' list (Settings.svelte): one column, its rows in one rounded block. */
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

  .row.open {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .row > :global(svg) {
    flex: none;
    color: var(--accent);
  }

  .row.urgent > :global(svg) {
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

  .chev {
    display: grid;
    flex: none;
    color: var(--muted);
  }
</style>
