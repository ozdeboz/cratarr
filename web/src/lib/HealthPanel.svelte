<script lang="ts">
  // A problem in the panel (the owner, 2026-10-23): one layout for every problem, filled from its data
  // (lib/healthReport's Problem), never laid out per problem: its domain › what it is; how it stands (critical, a
  // warning, to know) and since when; what it means; how much, as a meter where there is a whole; the things it is about,
  // one by one (the owner: "more deeper information": the albums waiting, the playlists and their missing tracks, the
  // folders not heard, the slow pages, the files), each opening where it is; and where to deal with it all. Beside the
  // list on a roomy window (no ✕ there), over it on a phone.
  import { findsTracks, offers } from './capabilities.svelte';
  import { glass, handGlass } from './glass.svelte';
  import GlobalResults from './GlobalResults.svelte';
  import { everywhere } from './globalSearch.svelte';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import { albumCoverUrl, wantAlbum, type HealthItem } from './api';
  import { toast } from './toast.svelte';
  import { openFinding } from './health';
  import { health, loadHealth, problems } from './healthReport.svelte';
  import HealthEvents from './HealthEvents.svelte';
  import HealthIncoming from './HealthIncoming.svelte';
  import HealthMissing from './HealthMissing.svelte';
  import HealthRenumber from './HealthRenumber.svelte';
  import HealthFixes from './HealthFixes.svelte';
  import { apply, FIXABLE, healthFixes, readFixes } from './fixes.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import AlbumMenu from './AlbumMenu.svelte';
  import ReplaceTrackSheet from './ReplaceTrackSheet.svelte';
  import { me } from './me.svelte';
  import ReleaseRow from './ReleaseRow.svelte';
  import { matchesSearch } from './pageView.svelte';
  import TopPill from './TopPill.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import { albumSubject, backFromStep, closePlaylist, HEALTH, healthItemOf, openStep, openTab, panel, upToPage, viewOver } from './panel.svelte';
  import Crumbs from './Crumbs.svelte';

  let { problem, beside = false }: { problem: string; beside?: boolean } = $props();
  const p = $derived(problems().find((x) => x.key === problem) ?? null);
  const f = $derived(p?.finding ?? null);
  const LEVEL = { critical: 'Critical', warning: 'Warning', info: 'To know' } as const;
  const WHERE: Record<string, string> = { library: 'Open the Library', playlists: 'Open Playlists', settings: 'Open Settings', health: 'Open Health', requests: 'Open Requests' };
  // The third level (the owner, 2026-10-23): an item's own page, a step deeper in the panel, back to the problem.
  const at = $derived(healthItemOf(panel.viewed));
  const item = $derived(p && at !== null ? (p.items[at] ?? null) : null);
  /** An Incoming album waiting (or not converted): compared with Music, its menu at hand (lib/HealthIncoming). */
  const incoming = $derived(problem.startsWith('incoming:'));
  /** The log events behind an item grouped from them (health.rs `grouped`): their area, event and grouping field. */
  const EVENTS: Record<string, { area: string; what: string; by: string }> = {
    'server:slow': { area: 'server', what: 'slow response', by: 'path' },
    'server:runaway': { area: 'server', what: 'client requests too often', by: 'client' },
    'sign-in:refused': { area: 'signin', what: 'request refused', by: 'path' },
    'sign-in:throttled': { area: 'signin', what: 'sign-in throttled', by: 'client' },
  };
  /** Where an item's page sends on: its album, its playlist. */
  /** The album whose Change cover is open (Health's albums without a cover). */
  let coverOf = $state<string | null>(null);
  /** The track whose better copy is being found (a file cut off or breaking off). */
  let betterOf = $state<string | null>(null);
  const onward = (x: HealthItem) =>
    x.album ? { label: 'Open the album', go: () => viewOver(albumSubject(x.album![0], x.album![1])) } : x.playlist ? { label: 'Open the playlist', go: () => openTab([x.playlist!], 'tracks') } : null;
  const since = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
  // On a phone this panel's glass is App's (lib/glass, the owner 2026-10-25: one glass, page's and panel's): handed over,
  // its own bar not drawn, its list under the glass.
  const handed = $derived(glass.one && !beside);
  const pill = () => (!beside && !item ? { device: false, filter: false, view: false, search: 'health' } : null);
  handGlass(() => handed, { left: glassLeft, pill });
  // An album missing tracks: the whole release asked for (Lidarr fetches it into Incoming; Replace carries over).
  let asking = $state(false);
  let askedFull = $state<string | null>(null);
  async function askFull(x: HealthItem) {
    if (!x.release_group || asking) return;
    asking = true;
    try {
      await wantAlbum({ release_group: x.release_group, artist: (x.line ?? '').split(' · ')[0], album: x.title });
      askedFull = x.release_group;
    } catch (e) {
      toast.fail(e, "The album wasn't asked for");
    } finally {
      asking = false;
    }
  }
  // A library finding's fixes (0.9.7, the administrators' metadata workflow): read for its list (Apply all certain) and
  // its items' pages (lib/HealthFixes).
  const fixable = $derived(me.admin && FIXABLE.has(problem));
  $effect(() => {
    if (fixable) readFixes(problem);
  });
  const certain = $derived(fixable ? (healthFixes[problem]?.certain ?? 0) : 0);
  let applyingAll = $state(false);
  let askAll = $state(false);
  async function applyAll() {
    askAll = false;
    applyingAll = true;
    await apply({ certain: problem }, problem, `Applied ${certain} certain ${certain === 1 ? 'fix' : 'fixes'}`);
    applyingAll = false;
  }
  // The levels over a problem: Health (not beside its list, which names it). Not the problem's domain: no level of its
  // own (it led to Health too), and often the problem's name again (the owner, 2026-10-26: "Health › Analysis ›
  // Analysis").
  const top = $derived(beside ? [] : [{ label: 'Health', go: () => upToPage('#/health') }]);
</script>

<!-- This panel's glass: its own bar's parts, or App's on a phone (lib/glass). -->
{#snippet glassLeft()}
  {#if item}
    <span class="navpill"><button aria-label="Back" title="Back" onclick={() => backFromStep(HEALTH + problem)}><Icon name="back" size={22} /></button></span>
  {:else if !beside}
    <span class="navpill"><button aria-label="Back" title="Back" onclick={() => closePlaylist()}><Icon name="back" size={22} /></button></span>
  {/if}
  {#if item}
    <Crumbs trail={[...top, ...(p ? [{ label: p.name, go: () => backFromStep(HEALTH + problem) }] : [])]} here={item.title} />
  {:else if p}
    <Crumbs trail={top} here={p.name} />
  {:else}
    <h2 class="title">Health</h2>
  {/if}
{/snippet}

<div class="view" class:handed>
  {#if !handed}
    <div class="bar" class:beside>
      {@render glassLeft()}
      {#if pill()}<TopPill {...pill()!} />{/if}
    </div>
  {/if}
  <div class="scroll" class:away={handed && everywhere()}>
    {#if item}
      <!-- An item's own page: what it is, and what can be done with it here. -->
      {@const on = onward(item)}
      {#if item.album && !incoming}<span class="big"><Cover src={albumCoverUrl(item.album[0], item.album[1])} /></span>{/if}
      {#if item.line}<p class="what">{item.line}</p>{/if}
      {#if item.playlist}<HealthMissing playlist={item.playlist} />{/if}
      {#if incoming && item.album}<HealthIncoming source={item.album[0]} album={item.album[1]} ongone={() => backFromStep(HEALTH + problem)} />{/if}
      <!-- Track numbers off (unnumbered ones too): renumbered as a release has them (lib/HealthRenumber). -->
      {#if problem === 'library:track-numbers' && item.album?.[0] === 'cratarr' && offers('renumber')}<HealthRenumber album={item.album[1]} title={item.title} ongone={() => backFromStep(HEALTH + problem)} />{/if}
      <!-- Year, genre, album artist, MusicBrainz release, a rule's names: the fix proposed, Apply (lib/HealthFixes). -->
      {#if fixable && item.album?.[0] === 'cratarr'}<HealthFixes finding={problem} album={item.album[1]} track={item.track ?? null} title={item.title} ongone={() => backFromStep(HEALTH + problem)} />{/if}
      <!-- No cover: Change cover, straight on its choices (Upload and an address in its corner). -->
      {#if problem === 'library:cover' && item.album?.[0] === 'cratarr' && me.admin}
        <button class="act" onclick={() => (coverOf = item.album![1])}>Change cover<Icon name="chevronRight" size={18} /></button>
        {#if coverOf === item.album[1]}
          <AlbumMenu source="cratarr" album={{ id: item.album[1], title: item.title, artist: null }} start="cover" onclose={() => ((coverOf = null), loadHealth())} />
        {/if}
      {/if}
      <!-- A track's file cut off or breaking off (2026-10-28): a better copy of it, as the track's menu finds one. -->
      {#if item.track && item.album && me.admin}
        {#if findsTracks()}<button class="act" onclick={() => (betterOf = item.track!)}>Find a better copy<Icon name="chevronRight" size={18} /></button>{/if}
        {#if betterOf === item.track}
          <ReplaceTrackSheet source={item.album[0]} album={{ id: item.album[1], title: '' }} replace={{ source: item.album[0], id: item.track }} onclose={() => ((betterOf = null), loadHealth())} />
        {/if}
      {/if}
      {#if EVENTS[problem]}<HealthEvents {...EVENTS[problem]} value={item.title} />{/if}
      {#if item.missing?.length}
        <!-- An album missing tracks (0.9.7; the owner, 2026-10-26: "golden work"): which, and the whole album asked for. -->
        <ul class="files">{#each item.missing as m (m)}<li>{m}</li>{/each}</ul>
        {#if item.release_group}
          {#if item.request || askedFull === item.release_group}
            <p class="muted">Asked for the full album.</p>
          {:else}
            <button class="act" disabled={asking} aria-busy={asking} onclick={() => askFull(item)}>Ask for the full album</button>
          {/if}
        {/if}
      {/if}
      {#if item.files?.length}
        <!-- A folder's files not heard yet, by name. -->
        <ul class="files">{#each item.files as f (f)}<li>{f}</li>{/each}</ul>
      {/if}
      {#if on}<button class="act" onclick={on.go}>{on.label}<Icon name="chevronRight" size={18} /></button>{/if}
    {:else if !p}
      {#if health.loaded}<p class="muted">This is no longer a problem.</p>{/if}
    {:else}
      <!-- How it stands. -->
      <p class="state" class:urgent={p.urgent}>
        <Icon name={p.icon} size={20} />
        <span>{f ? LEVEL[f.level] : p.urgent ? 'Critical' : 'Warning'}{#if f?.since}<span class="muted"> · since {since(f.since)}</span>{/if}</span>
      </p>
      <!-- What it means. -->
      {#if f?.detail || p.hint}<p class="what">{f?.detail ?? p.hint}</p>{/if}
      <!-- How much. -->
      {#if f && f.of}
        <div class="meter" role="meter" aria-valuemin="0" aria-valuemax={f.of} aria-valuenow={f.count} aria-label={p.title}>
          <!-- What it is, in its sentence (the owner, 2026-10-25), over how much of the whole. -->
          <div class="head"><span>{p.said}</span><b>{Math.round((100 * f.count) / f.of)}%</b></div>
          <div class="track"><i style:width={`${(100 * f.count) / f.of}%`}></i></div>
        </div>
      {:else}
        <p class="count"><b>{p.said}</b></p>
      {/if}
      <!-- Every certain fix of it at once (the owner, 2026-10-26), asked once more; likely ones one by one, on their pages. -->
      {#if certain > 0}
        <button class="act" disabled={applyingAll} aria-busy={applyingAll} onclick={() => (askAll = true)}>Apply all certain ({certain.toLocaleString()})</button>
      {/if}
      <!-- The things it is about, one by one. -->
      {#if p.items.length}
        <!-- An album a release row, as in every one of these panels (lib/ReleaseRow); anything else a settings row. -->
        <Group>
          {#each p.items as x, i (i)}
            {#if !matchesSearch(x.title, x.line)}<!-- left out by the search -->
            {:else if x.album}
              <ReleaseRow cover={albumCoverUrl(x.album[0], x.album[1])} title={x.title} line={x.line ?? null} onclick={() => openStep(`${HEALTH}${problem}/${i}`)} />
            {:else}
              <Item name={x.title} line={x.line ?? null} onclick={() => openStep(`${HEALTH}${problem}/${i}`)} />
            {/if}
          {/each}
        </Group>
      {/if}
      <!-- The way to deal with it. -->
      {#if f?.open}
        <button class="act" onclick={() => f && openFinding(f)}>{WHERE[f.open.page] ?? 'Open'}<Icon name="chevronRight" size={18} /></button>
      {/if}
    {/if}
      <!-- On a phone, over the page: the global results at the end of this list (decision 0072). -->
    {#if handed && everywhere()}<GlobalResults />{/if}
  </div>
</div>

{#if askAll}
  <ConfirmSheet
    title={`Apply ${certain.toLocaleString()} certain ${certain === 1 ? 'fix' : 'fixes'}?`}
    text="Written into the files; Undo puts the old values back."
    label="Apply"
    icon="check"
    onconfirm={applyAll}
    onclose={() => (askAll = false)}
  />
{/if}

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

  .muted {
    color: var(--muted);
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
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
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


  .files {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .files li {
    padding: 8px 0;
    border-top: 1px solid var(--border);
    overflow-wrap: anywhere;
  }

  .big {
    display: block;
    width: min(240px, 60%);
    aspect-ratio: 1;
    overflow: hidden;
    border-radius: 12px;
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

  /* Its glass handed to App's row on a phone (lib/glass): the list starts under it, its first heading where the page's
     is (the glass's room, then its own 16 px; it began 12 px higher, the owner 2026-10-25). */
  .view.handed .scroll {
    padding-top: calc(var(--bar-room, 0px) + var(--mini-top, 0px) + var(--glass-top) + 16px);
    padding-bottom: calc(var(--mini-h, 0px) + var(--glass-bottom) + 16px);
  }
</style>
