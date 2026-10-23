<script lang="ts">
  // A problem in the panel (the owner, 2026-10-23): one layout for every problem, filled from its data
  // (lib/healthReport's Problem), never laid out per problem: its domain › what it is; how it stands (needs you, a
  // warning, to know) and since when; what it means; how much, as a meter where there is a whole; the things it is about,
  // one by one (the owner: "more deeper information": the albums waiting, the playlists and their missing tracks, the
  // folders not heard, the slow pages, the files), each opening where it is; and where to deal with it all. Beside the
  // list on a roomy window (no ✕ there), over it on a phone.
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import { albumCoverUrl, type HealthItem } from './api';
  import { openFinding } from './health';
  import { health, problems } from './healthReport.svelte';
  import { albumSubject, closePlaylist, openTab, viewOver } from './panel.svelte';

  let { problem, beside = false }: { problem: string; beside?: boolean } = $props();
  const p = $derived(problems().find((x) => x.key === problem) ?? null);
  const f = $derived(p?.finding ?? null);
  const LEVEL = { needs_you: 'Needs you', warning: 'Warning', info: 'To know' } as const;
  const WHERE: Record<string, string> = { library: 'Open the Library', playlists: 'Open Playlists', settings: 'Open Settings', health: 'Open Health' };
  /** Where an item opens: its album, its playlist's Repair tab; none for the rest. */
  const opener = (x: HealthItem) =>
    x.album ? () => viewOver(albumSubject(x.album![0], x.album![1])) : x.playlist ? () => openTab([x.playlist!], 'repair') : null;
  const since = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
</script>

<div class="view">
  <div class="bar" class:beside>
    {#if !beside}<span class="navpill"><button aria-label="Close" title="Close" onclick={() => closePlaylist()}><Icon name="close" size={22} /></button></span>{/if}
    <h2 class="title">{#if p}<span class="up">{p.domain}</span><span class="sep" aria-hidden="true">›</span>{p.title}{:else}Health{/if}</h2>
  </div>
  <div class="scroll">
    {#if !p}
      {#if health.loaded}<p class="muted">This is no longer a problem.</p>{/if}
    {:else}
      <!-- How it stands. -->
      <p class="state" class:urgent={p.urgent}>
        <Icon name={p.icon} size={20} />
        <span>{f ? LEVEL[f.level] : p.urgent ? 'Needs you' : 'Warning'}{#if f?.since}<span class="muted"> · since {since(f.since)}</span>{/if}</span>
      </p>
      <!-- What it means. -->
      {#if f?.detail || p.hint}<p class="what">{f?.detail ?? p.hint}</p>{/if}
      <!-- How much. -->
      {#if f && f.of}
        <div class="meter" role="meter" aria-valuemin="0" aria-valuemax={f.of} aria-valuenow={f.count} aria-label={p.title}>
          <div class="head"><span>{f.count.toLocaleString()} of {f.of.toLocaleString()}</span><b>{Math.round((100 * f.count) / f.of)}%</b></div>
          <div class="track"><i style:width={`${(100 * f.count) / f.of}%`}></i></div>
        </div>
      {:else if f && f.count > 1}
        <p class="count"><b>{f.count.toLocaleString()}</b></p>
      {/if}
      <!-- The things it is about, one by one. -->
      {#if p.items.length}
        <ul class="items">
          {#each p.items as x, i (i)}
            {@const go = opener(x)}
            <li>
              <svelte:element this={go ? 'button' : 'div'} class="item" role={go ? undefined : 'listitem'} onclick={go ?? undefined}>
                {#if x.album}<span class="cover"><Cover src={albumCoverUrl(x.album[0], x.album[1])} /></span>{/if}
                <span class="words"><b>{x.title}</b>{#if x.line}<small>{x.line}</small>{/if}</span>
                {#if go}<Icon name="chevronRight" size={18} />{/if}
              </svelte:element>
            </li>
          {/each}
        </ul>
      {/if}
      <!-- The way to deal with it. -->
      {#if f?.open}
        <button class="act" onclick={() => f && openFinding(f)}>{WHERE[f.open.page] ?? 'Open'}<Icon name="chevronRight" size={18} /></button>
      {/if}
    {/if}
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

  .up,
  .sep,
  .muted {
    color: var(--muted);
  }

  .sep {
    margin: 0 8px;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: 16px;
    padding: 16px clamp(12px, 2vw, 24px) calc(var(--mini-h, 0px) + 16px);
  }

  p {
    margin: 0;
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

  .state {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 500;
  }

  .state :global(svg) {
    color: var(--accent);
  }

  .state.urgent :global(svg) {
    color: var(--danger);
  }

  .what {
    line-height: 1.5;
  }

  .meter .head {
    display: flex;
    justify-content: space-between;
    margin-bottom: 6px;
    font-variant-numeric: tabular-nums;
  }

  .track {
    height: 8px;
    overflow: hidden;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .track i {
    display: block;
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
  }

  .count b {
    font-size: 1.6rem;
    font-weight: 650;
  }

  h3 {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }

  .items {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* An item: its cover where it is an album, its words, › where it opens. */
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 52px;
    padding: 6px 0;
    border: 0;
    border-top: 1px solid var(--border);
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
  }

  button.item {
    cursor: pointer;
  }

  .item > :global(svg) {
    flex: none;
    color: var(--muted);
  }

  .cover {
    flex: none;
    width: 44px;
    height: 44px;
    overflow: hidden;
    border-radius: 6px;
  }

  .words {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .words b {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .words small {
    overflow: hidden;
    color: var(--muted);
    font-size: 0.85rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .act {
    justify-self: start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px 10px 18px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
</style>
