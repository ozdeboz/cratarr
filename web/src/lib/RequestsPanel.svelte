<script lang="ts">
  // A list of Requests in the panel (the owner, 2026-10-24: the page an index, as Downloads, Insights, Health and
  // Settings; the lists in panels): opened from the page's index, beside it on a roomy window (no ✕ there), over it on a
  // phone. Its bar: where it is (Requests › Your requests). Waiting for an answer (those who answer requests): each
  // answered from its row's menu, or many at once in the select mode every list has (the bar's icon starts it). Your
  // requests: each with where it stands, a waiting one taken back from its menu. Lidarr monitor: what it
  // fetches now (how far), then what it watches for and has not found yet.
  import { glass, handGlass } from './glass.svelte';
  import GlobalResults from './GlobalResults.svelte';
  import type { AlbumRequest, WatchedAlbum } from './api';
  import Icon from './Icon.svelte';
  import { me } from './me.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Notice from './Notice.svelte';
  import PageLoading from './PageLoading.svelte';
  import ReleaseRow from './ReleaseRow.svelte';
  import { matchesSearch } from './pageView.svelte';
  import TopPill from './TopPill.svelte';
  import { closePlaylist, upToPage } from './panel.svelte';
  import Crumbs from './Crumbs.svelte';
  import { answersRequests, decide, loadRequests, lookAtLidarr, REQUESTS_LISTS, requests, takeBack, type RequestsList } from './requests.svelte';
  import SelectBar from './SelectBar.svelte';
  import SelectSheet from './SelectSheet.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import ValueSheet from './settings/ValueSheet.svelte';
  import { when } from './settings/when';

  let { list, beside = false }: { list: RequestsList; beside?: boolean } = $props();

  const L = $derived(REQUESTS_LISTS.find((x) => x.id === list) ?? REQUESTS_LISTS[1]);
  $effect(() => void loadRequests());
  $effect(() => {
    if (list !== 'lidarr') return;
    lookAtLidarr();
    const t = setInterval(lookAtLidarr, 30_000);
    return () => clearInterval(t);
  });

  const view = $derived(requests.view);
  const answers = $derived(answersRequests());

  const STATUS = { pending: 'Waiting', approved: 'Approved', declined: 'Declined', cancelled: 'Taken back' } as const;
  /** What a row says: the artist, then where it stands (and who asked, among everyone's). */
  function line(r: AlbumRequest, who: boolean): string {
    const parts = [r.artist];
    if (who) parts.push(`asked by ${r.user}`);
    parts.push(standing(r));
    return parts.join(' · ');
  }
  function standing(r: AlbumRequest): string {
    const a = r.album_state;
    if (a?.state === 'in_library') return 'In the library';
    if (r.status === 'declined') return r.reason ? `Declined: ${r.reason}` : 'Declined';
    if (r.status !== 'approved') return `${STATUS[r.status]} · ${when(r.at)}`;
    if (r.fetched?.startsWith('Not handed over')) return r.fetched;
    switch (a?.state) {
      case 'fetching':
        return `Being fetched · ${Math.round(a.done * 100)}%`;
      case 'in_incoming':
        return 'In Incoming';
      case 'partly_there':
        return `Partly there · ${a.tracks} of ${a.of}`;
      case 'looking':
        return 'Lidarr is looking for it';
      default:
        return r.fetched ?? 'Approved';
    }
  }
  const tone = (r: AlbumRequest) => (r.status === 'declined' || r.fetched?.startsWith('Not handed over') ? 'bad' : null);
  const fill = (r: AlbumRequest) => (r.album_state?.state === 'fetching' ? r.album_state.done : null);
  /** A release group's cover, as Discover's (the Cover Art Archive). */
  const coverOf = (w: WatchedAlbum) => (w.release_group ? `https://coverartarchive.org/release-group/${w.release_group}/front-250` : null);
  const watchLine = (w: WatchedAlbum) =>
    [w.artist, w.year, w.done !== null ? `${Math.round(w.done * 100)}%${w.status && w.status !== 'downloading' ? ` · ${w.status}` : ''}` : null]
      .filter(Boolean)
      .join(' · ');

  // --- A row's menu ---
  let opened = $state<AlbumRequest | null>(null);
  const canTakeBack = (r: AlbumRequest) => (r.status === 'pending' && (r.user === me.name || answers)) || (r.status === 'approved' && answers);
  let declining = $state<number[] | null>(null);
  function answer(ids: number[], approve: boolean, reason: string | null = null) {
    selecting = null;
    decide(ids, approve, reason);
  }

  // --- Select (everyone's waiting ones) ---
  let selecting = $state<Set<number> | null>(null);
  let selectSheet = $state(false);
  // The pill's search (the owner, 2026-10-25): a request whose album, artist or asker holds every word.
  const found = (r: AlbumRequest) => matchesSearch(r.album, r.artist, r.user);
  const waiting = $derived((view?.waiting ?? []).filter(found));
  const mine = $derived((view?.mine ?? []).filter(found));
  const ticked = $derived(selecting ? waiting.filter((r) => selecting!.has(r.id)).map((r) => r.id) : []);
  function tick(id: number) {
    if (!selecting) return;
    const s = new Set(selecting);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    selecting = s;
  }
  function selectWhich(which: 'all' | 'none' | 'others') {
    const all = waiting.map((r) => r.id);
    selecting = new Set(which === 'all' ? all : which === 'none' ? [] : all.filter((i) => !selecting?.has(i)));
  }
  $effect(() => {
    void list;
    selecting = null;
  });
  // On a phone this panel's glass is App's (lib/glass, the owner 2026-10-25: one glass, page's and panel's): handed over,
  // its own bar not drawn, its list under the glass.
  const handed = $derived(glass.one && !beside);
  const pill = () => (!beside ? { device: false, filter: false, view: false, search: 'requests' } : null);
  handGlass(() => handed, { left: glassLeft, pill, extra: glassExtra });
</script>

<!-- This panel's glass: its own bar's parts, or App's on a phone (lib/glass). -->
{#snippet glassLeft()}
  {#if !beside}<span class="navpill"><button aria-label="Close" title="Close" onclick={() => closePlaylist()}><Icon name="close" size={22} /></button></span>{/if}
  <Crumbs up={beside ? null : { label: 'Requests', go: () => upToPage('#/requests') }} here={L.label} />
{/snippet}
{#snippet glassExtra()}
  {#if list === 'waiting' && waiting.length}
    <span class="navpill"
      ><button aria-label="Select" title="Select" aria-pressed={!!selecting} onclick={() => (selecting = selecting ? null : new Set())}><Icon name="listChecks" size={22} /></button></span
    >
  {/if}
{/snippet}

{#snippet row(r: AlbumRequest, who: boolean, tickable: boolean)}
  {#if tickable && selecting}
    <ReleaseRow cover={r.cover} title={r.album} line={line(r, who)} tone={tone(r)}>
      <button class="tick" class:on={selecting.has(r.id)} role="checkbox" aria-checked={selecting.has(r.id)} aria-label={`Select ${r.album}`} onclick={() => tick(r.id)}
        ><Icon name={selecting.has(r.id) ? 'circleCheck' : 'circle'} size={24} /></button
      >
    </ReleaseRow>
  {:else}
    <ReleaseRow cover={r.cover} title={r.album} line={line(r, who)} tone={tone(r)} fill={fill(r)} onclick={() => (opened = r)} />
  {/if}
{/snippet}

<div class="view" class:handed>
  {#if !handed}
    <div class="bar" class:beside>
      {@render glassLeft()}
      {#if pill()}<TopPill {...pill()!} />{/if}
      {@render glassExtra()}
    </div>
  {/if}
  <div class="scroll">
    {#if requests.error && !view}
      <Notice text={requests.error} />
    {:else if !view}
      <PageLoading />
    {:else if list === 'waiting'}
      {#if !view.fetcher}<p class="note">Approved requests need Lidarr: <a href="#/settings?view=settings%3Aconnections%2Flidarr">connect it</a>.</p>{/if}
      <Group>
        {#each waiting as r (r.id)}{@render row(r, true, true)}{:else}<Item name="None waiting" />{/each}
      </Group>
    {:else if list === 'mine'}
      <Group>
        {#each mine as r (r.id)}{@render row(r, false, false)}{:else}
          <Item name="None yet" line="Want an album in Discover to ask for it." onclick={() => (location.hash = '#/discover')} />
        {/each}
      </Group>
    {:else if !requests.watching}
      {#if view.fetcher}<PageLoading />{:else}<p class="note">Lidarr is not connected: <a href="#/settings?view=settings%3Aconnections%2Flidarr">connect it</a>.</p>{/if}
    {:else}
      {@const w = requests.watching}
      {#if w.fetching.length}
        <Group title="Fetching now">
          {#each w.fetching.filter((a) => matchesSearch(a.album, a.artist)) as a (`f:${a.release_group ?? a.album}`)}<ReleaseRow cover={coverOf(a)} title={a.album} line={watchLine(a)} fill={a.done} />{/each}
        </Group>
      {/if}
      <Group title="Not found yet" more="What Lidarr watches for and has not found yet (its Wanted).">
        {#each w.looking.filter((a) => matchesSearch(a.album, a.artist)) as a (`l:${a.release_group ?? a.album}`)}<ReleaseRow cover={coverOf(a)} title={a.album} line={watchLine(a)} />{:else}<Item name="Nothing" />{/each}
        {#if w.more}<Item name="{w.more.toLocaleString()} more" line="In Lidarr's Wanted" />{/if}
      </Group>
    {/if}
      <!-- On a phone, over the page: the global results at the end of this list (decision 0072). -->
    {#if handed}<GlobalResults />{/if}
  </div>
</div>

{#if selecting}
  <SelectBar label="Selected requests" count="{ticked.length}/{waiting.length}" onclose={() => (selecting = null)}>
    <button aria-label="Select which" title="Select…" onclick={() => (selectSheet = true)}><Icon name="listChecks" size={24} /></button>
    <button aria-label="Approve" title="Approve" disabled={!ticked.length} onclick={() => answer(ticked, true)}><Icon name="check" size={24} /></button>
    <button aria-label="Decline" title="Decline" disabled={!ticked.length} onclick={() => (declining = ticked)}><Icon name="ban" size={24} /></button>
  </SelectBar>
{/if}
{#if selectSheet}
  <SelectSheet onall={() => selectWhich('all')} onnone={() => selectWhich('none')} onothers={() => selectWhich('others')} onclose={() => (selectSheet = false)} />
{/if}

{#if opened}
  {@const r = opened}
  <MenuSheet
    label={r.album}
    title={r.album}
    images={r.cover ? [r.cover] : []}
    chips={[r.artist, ...(r.year ? [String(r.year)] : [])]}
    copy={`${r.artist} – ${r.album}`}
    onclose={() => (opened = null)}
  >
    <div role="menu">
      {#if answers && (r.status === 'pending' || (r.status === 'approved' && r.fetched?.startsWith('Not handed over')))}
        <MenuEntry icon="check" label={r.status === 'pending' ? 'Approve' : 'Hand over again'} onclick={() => (answer([r.id], true), (opened = null))} />
      {/if}
      {#if answers && r.status === 'pending'}
        <MenuEntry icon="ban" label="Decline…" onclick={() => ((declining = [r.id]), (opened = null))} />
      {/if}
      {#if canTakeBack(r)}
        <MenuEntry icon="undo" label="Take back" danger onclick={() => (takeBack(r), (opened = null))} />
      {/if}
      {#if !answers && !canTakeBack(r)}
        <MenuEntry icon="compass" label="Discover" onclick={() => ((opened = null), (location.hash = '#/discover'))} />
      {/if}
    </div>
  </MenuSheet>
{/if}

{#if declining}
  {@const ids = declining}
  <ValueSheet
    title={ids.length === 1 ? 'Decline' : `Decline ${ids.length}`}
    value=""
    placeholder="Why (optional)"
    saveLabel="Decline"
    onsave={(why: string) => ((declining = null), answer(ids, false, why.trim() || null))}
    onclose={() => (declining = null)}
  />
{/if}

<style>
  /* As Insights' view in the panel (lib/InsightsPanel): its bar on top, what it holds under it. */
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

  .scroll {
    display: grid;
    flex: 1;
    align-content: start;
    gap: 24px;
    min-height: 0;
    overflow-y: auto;
    padding: 16px clamp(12px, 2vw, 24px) calc(var(--mini-h, 0px) + 16px);
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 14px;
  }

  .note a {
    color: var(--accent-ink);
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

  .navpill button[aria-pressed='true'] {
    color: var(--accent);
  }

  .tick {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .tick.on {
    color: var(--accent);
  }

  /* Its glass handed to App's row on a phone (lib/glass): the list starts under it. */
  .view.handed .scroll {
    padding-top: 64px;
  }
</style>
