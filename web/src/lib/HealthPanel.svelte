<script lang="ts">
  // A part of Health in the panel (the owner, 2026-10-23: "the same layout makeover as our settings page"): one domain's
  // findings, worst first, each opening where it is dealt with; Files' with the files themselves, by kind, as folded
  // sections of file rows. Beside the list on a roomy window a part is always open (no ✕ there), over it on a phone.
  import FileItem from './FileItem.svelte';
  import Icon from './Icon.svelte';
  import { DOMAINS, iconOf, openFinding } from './health';
  import { FILE_KINDS, health } from './healthReport.svelte';
  import type { HealthFinding } from './api';
  import { closePlaylist } from './panel.svelte';
  import Section from './Section.svelte';

  let { domain, beside = false }: { domain: string; beside?: boolean } = $props();
  const list = $derived(health.findings.filter((f) => f.domain === domain));
  const files = $derived(domain === 'files' ? FILE_KINDS.map((k) => ({ ...k, list: (health.problems ?? []).filter((p) => p.kind === k.kind) })).filter((k) => k.list.length) : []);
</script>

{#snippet words(f: HealthFinding)}
  <span><b>{f.title}</b>{#if f.detail}<small>{f.detail}</small>{/if}</span>
{/snippet}

<div class="view">
  <div class="bar" class:beside>
    {#if !beside}<span class="navpill"><button aria-label="Close" title="Close" onclick={() => closePlaylist()}><Icon name="close" size={22} /></button></span>{/if}
    <h2 class="title">{DOMAINS[domain]?.name ?? 'Health'}</h2>
  </div>
  <div class="scroll">
    {#if list.length && domain !== 'files'}
      <ul class="findings">
        {#each list as f (f.key)}
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
    {/if}
    {#each files as k (k.kind)}
      <Section title={k.title} count={`${k.list.length} ${k.list.length === 1 ? 'file' : 'files'}`} hint={k.hint} open={false}>
        <ul class="findings">
          {#each k.list as p (p.folder + p.rel)}<li><FileItem path={p.rel} detail={p.detail ?? undefined} /></li>{/each}
        </ul>
      </Section>
    {/each}
    {#if !files.length && (!list.length || domain === 'files')}<p class="empty">All good.</p>{/if}
  </div>
</div>

<style>
  /* As a part of the settings in the panel (lib/SettingsPanel): its bar on top, what it holds under it. */
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 65px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .bar.beside {
    padding-left: clamp(12px, 2vw, 24px);
  }

  .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 20px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px clamp(12px, 2vw, 24px) calc(var(--mini-h, 0px) + 16px);
  }

  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--text);
    cursor: pointer;
  }

  .empty {
    margin: 16px 4px;
    color: var(--muted);
  }

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
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>
