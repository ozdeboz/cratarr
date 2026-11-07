<script lang="ts">
  // A list of Requests in the panel (the owner, 2026-10-24: the page an index, as Downloads, Insights, Health and
  // Settings; the lists in panels): opened from the page's index, beside it on a roomy window (no ✕ there), over it on a
  // phone. Its bar: where it is (Requests › Your requests). Waiting for an answer (those who answer requests): each
  // answered from its row's menu, or many at once in the select mode every list has (the bar's icon starts it). Your
  // requests: each with where it stands, a waiting one taken back from its menu, or many at once in Select (the owner,
  // 2026-10-27: what one has, several have). Monitor (the owner, 2026-10-26: not Lidarr's name, so another downloader fits): what it
  // fetches now (how far), then what it watches for and has not found yet.
  import ChoiceMark from './ChoiceMark.svelte';
  import { askIfMany, fewAtATime } from './many.svelte';
  import { rowRoot } from './glassRow.svelte';
  import { hrefOf, viewOn } from './addresses';
  import { toast } from './toast.svelte';
  import { plural } from './insights.svelte';
  import { glass, handGlass } from './glass.svelte';
  import GlobalResults from './GlobalResults.svelte';
  import { everywhere } from './globalSearch.svelte';
  import { cancelRequest, type AlbumRequest, type WatchedAlbum } from './api';
  import Icon from './Icon.svelte';
  import { me } from './me.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Notice from './Notice.svelte';
  import PageLoading from './PageLoading.svelte';
  import ReleaseRow from './ReleaseRow.svelte';
  import { matchesSearch, searchNone } from './pageView.svelte';
  import ContentControls from './ContentControls.svelte';
  import { backOne, REQUESTS, upToPage, viewPlaylist, viewSibling } from './panel.svelte';
  import Crumbs from './Crumbs.svelte';
  import { answersRequests, decide, listsShown, loadRequests, lookAtLidarr, REQUESTS_LISTS, requests, takeBack, type RequestsList } from './requests.svelte';
  import ChoiceList from './ChoiceList.svelte';
  import { clearPlaced, dropSoulseek, fetchFound, loadSoulseek, searchSoulseekNow, soulseek } from './soulseek.svelte';
  import { approvePayload, folderLine, GROUPS, itemLine, itemTone, searchable, tickable, underWay, type SoulseekItem } from './soulseekRules';
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

  // Soulseek (decision 0099): read while it shows, every few seconds while a search or a fetch is under way, else every
  // half minute (Lidarr's Wanted is read every six hours, a search may find something any time). Found records ticked
  // here, none until the administrator ticks them; Fetch sends the ticked ones, each from the folder picked for it.
  $effect(() => {
    if (list !== 'soulseek') return;
    let gone = false;
    let t: ReturnType<typeof setTimeout>;
    const again = () => loadSoulseek().then(() => !gone && (t = setTimeout(again, underWay(soulseek.view) ? 3000 : 30_000)));
    void again();
    return () => ((gone = true), clearTimeout(t));
  });
  let slskTicks = $state(new Set<string>());
  let slskPicks = $state<Record<string, number>>({});
  let slskOpen = $state<SoulseekItem | null>(null);
  let fetching = $state(false);
  const slskItems = $derived(soulseek.view?.items ?? []);
  const slskFound = $derived(slskItems.filter(tickable));
  const slskPayload = $derived(approvePayload(slskItems, slskTicks, slskPicks));
  // A tick kept only where it can still be (a record fetched or gone loses it).
  $effect(() => {
    const can = new Set(slskFound.map((i) => i.id));
    if ([...slskTicks].some((id) => !can.has(id))) slskTicks = new Set([...slskTicks].filter((id) => can.has(id)));
  });
  function slskTick(id: string) {
    const next = new Set(slskTicks);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    slskTicks = next;
  }
  async function fetchTicked() {
    if (!slskPayload.length || fetching) return;
    fetching = true;
    await fetchFound(slskPayload);
    slskTicks = new Set();
    fetching = false;
  }
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
  // Many approved at once ask first (lib/many; declining many asks already, for why).
  function answer(ids: number[], approve: boolean, reason: string | null = null) {
    const go = () => {
      selecting = null;
      decide(ids, approve, reason);
    };
    if (!approve) return go();
    const n = plural(ids.length, 'request');
    askIfMany(ids.length, { title: `Approve ${n}?`, label: `Approve ${n}`, icon: 'check' }, go);
  }

  // --- Select (everyone's waiting ones; one's own that can be taken back) ---
  let selecting = $state<Set<number> | null>(null);
  let selectSheet = $state(false);
  // The pill's search (the owner, 2026-10-25): a request whose album, artist or asker holds every word.
  const found = (r: AlbumRequest) => matchesSearch(r.album, r.artist, r.user);
  const waiting = $derived((view?.waiting ?? []).filter(found));
  const mine = $derived((view?.mine ?? []).filter(found));
  const seen = (a: { album: string; artist: string | null }) => matchesSearch(a.album, a.artist);
  // The page's words leaving none of the list (decision 0095): said from the magnifier, not the list's "None waiting".
  searchNone(() => {
    if (list === 'waiting') return !waiting.length && { n: view?.waiting?.length ?? 0, one: 'request' };
    if (list === 'mine') return !mine.length && { n: view?.mine.length ?? 0, one: 'request' };
    const w = requests.watching;
    return !!w && !w.fetching.some(seen) && !w.looking.some(seen) && { n: w.fetching.length + w.looking.length, one: 'record' };
  }, 'panel');
  /** What Select can tick in this list. */
  const selectable = $derived(list === 'waiting' ? waiting : list === 'mine' ? mine.filter(canTakeBack) : []);
  const ticked = $derived(selecting ? selectable.filter((r) => selecting!.has(r.id)).map((r) => r.id) : []);
  function tick(id: number) {
    if (!selecting) return;
    const s = new Set(selecting);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    selecting = s;
  }
  function selectWhich(which: 'all' | 'none' | 'others') {
    const all = selectable.map((r) => r.id);
    selecting = new Set(which === 'all' ? all : which === 'none' ? [] : all.filter((i) => !selecting?.has(i)));
  }
  $effect(() => {
    void list;
    selecting = null;
  });
  /** The ticked ones of one's own taken back, as one is from its menu; many ask first (lib/many). */
  function takeBackTicked() {
    const ids = [...ticked];
    const n = plural(ids.length, 'request');
    askIfMany(ids.length, { title: `Take back ${n}?`, label: `Take back ${n}`, icon: 'undo' }, async () => {
      selecting = null;
      const failed = await fewAtATime(ids, cancelRequest);
      if (failed.length) toast.fail(null, failed.length === ids.length ? 'Could not take them back' : "Not all were taken back");
      loadRequests();
    });
  }
  // On a phone this panel's glass is App's (lib/glass, the owner 2026-10-25: one glass, page's and panel's): handed over,
  // its own bar not drawn, its list under the glass.
  const handed = $derived(glass.one && !beside);
  const pill = () => (!beside ? { filter: false, view: false, search: 'requests' } : null);
  handGlass(() => handed, { left: glassLeft, pill, extra: glassExtra });
</script>

<!-- This panel's glass: its own bar's parts, or App's on a phone (lib/glass). -->
{#snippet glassLeft()}
  <Crumbs
    back={beside ? null : backOne}
    trail={beside ? [] : [{ label: 'Requests', go: () => upToPage('requests') }]}
    here={L.name}
    siblings={beside ? null : { menuLabel: 'Requests shows', options: listsShown(answersRequests(), me.admin, view?.fetchers ?? []).map((l) => ({ id: l.id, label: l.name })), chosen: L.id, onchoose: (id) => viewSibling(REQUESTS + id) }}
  />
{/snippet}
{#snippet glassExtra()}
  {#if selectable.length}
    <span class="navpill"
      ><button aria-label="Select" title="Select" aria-pressed={!!selecting} onclick={() => (selecting = selecting ? null : new Set())}><Icon name="listChecks" size={22} /></button></span
    >
  {/if}
{/snippet}

{#snippet row(r: AlbumRequest, who: boolean, tickable: boolean)}
  {#if tickable && selecting}
    <ReleaseRow cover={r.cover} title={r.album} line={line(r, who)} tone={tone(r)}>
      <button class="tick" class:on={selecting.has(r.id)} role="checkbox" aria-checked={selecting.has(r.id)} aria-label={`Select ${r.album}`} onclick={() => tick(r.id)}
        ><ChoiceMark kind="several" on={selecting.has(r.id)} /></button
      >
    </ReleaseRow>
  {:else}
    <ReleaseRow cover={r.cover} title={r.album} line={line(r, who)} tone={tone(r)} fill={fill(r)} onclick={() => (opened = r)} />
  {/if}
{/snippet}

<div class="view" class:handed>
  {#if !handed}
    <div class="bar" {@attach rowRoot()} class:beside>
      {@render glassLeft()}
      {#if pill()}<ContentControls {...pill()!} />{/if}
      {@render glassExtra()}
    </div>
  {/if}
  <div class="scroll" class:away={handed && everywhere()}>
    {#if requests.error && !view}
      <Notice text={requests.error} onretry={loadRequests} />
    {:else if !view}
      <PageLoading />
    {:else if list === 'waiting'}
      {#if !view.fetchers.length}<p class="note">Approved requests need a fetcher, Lidarr or Soulseek: {#if me.admin}<a href={viewOn('settings', 'settings:connections')}>connect one</a>{:else}an administrator can connect one{/if}.</p>{/if}
      <Group>
        {#each waiting as r (r.id)}{@render row(r, true, true)}{:else}{#if !view.waiting?.length}<Item name="None waiting" />{/if}{/each}
      </Group>
    {:else if list === 'soulseek'}
      {#if soulseek.error && !soulseek.view}
        <Notice text={soulseek.error} onretry={loadSoulseek} />
      {:else if !soulseek.view}
        <PageLoading />
      {:else}
        {#if !soulseek.view.library}
          <!-- Nothing is searched until an Incoming library takes Soulseek's records (decision 0099). -->
          <p class="note">Soulseek's records need an Incoming library to come into: choose one in <a href={viewOn('settings', 'settings:libraries')}>Libraries</a>.</p>
        {/if}
        {#if slskFound.length}
          <Group>
            <Item name={`${slskTicks.size} of ${slskFound.length} ticked`}>
              <button onclick={() => (slskTicks = slskTicks.size === slskFound.length ? new Set() : new Set(slskFound.map((i) => i.id)))}
                >{slskTicks.size === slskFound.length ? 'None' : 'Tick all found'}</button
              >
              <button class="primary" disabled={!slskPayload.length || fetching} aria-busy={fetching || undefined} onclick={() => void fetchTicked()}
                >{slskPayload.length ? `Fetch ${slskPayload.length}` : 'Fetch'}</button
              >
            </Item>
          </Group>
        {/if}
        {#each GROUPS as g (g.title)}
          {@const these = slskItems.filter((i) => g.states.includes(i.state) && matchesSearch(i.record.album, i.record.artist))}
          {#if these.length}
            <Group title={g.title}>
              {#snippet tail()}{#if g.states.includes('placed')}<button class="tailact" onclick={() => void clearPlaced()}>Clear</button>{/if}{/snippet}
              {#each these as i (i.id)}
                <ReleaseRow cover={null} title={i.record.album} line={itemLine(i)} tone={itemTone(i)} fill={i.state === 'fetching' ? i.percent / 100 : undefined} onclick={() => (slskOpen = i)}>
                  {#if tickable(i)}
                    {@const on = slskTicks.has(i.id)}
                    <button class="tick" class:on role="checkbox" aria-checked={on} aria-label={`Tick ${i.record.album}`} onclick={() => slskTick(i.id)}
                      ><ChoiceMark kind="several" on={on} /></button
                    >
                  {:else if searchable(i)}
                    <button onclick={() => void searchSoulseekNow(i.id)}>Search now</button>
                  {/if}
                </ReleaseRow>
              {/each}
            </Group>
          {/if}
        {/each}
        {#if !slskItems.length}<Group><Item name="Nothing asked for" line="Requests you approve come here" /></Group>{/if}
      {/if}
    {:else if list === 'mine'}
      <Group>
        {#each mine as r (r.id)}{@render row(r, false, canTakeBack(r))}{:else}
          {#if !view.mine.length}<Item name="None yet" line="Want an album in Discover to ask for it." onclick={() => (location.hash = hrefOf('discover'))} />{/if}
        {/each}
      </Group>
    {:else if !requests.watching}
      {#if requests.watchError}<Notice text={requests.watchError} onretry={lookAtLidarr} />{:else if view.fetchers.includes('lidarr')}<PageLoading />{:else}<p class="note">Lidarr is not connected: {#if me.admin}<a href={viewOn('settings', 'settings:connections/lidarr')}>connect it</a>{:else}an administrator can connect it{/if}.</p>{/if}
    {:else}
      {@const w = requests.watching}
      {#if w.fetching.length}
        <Group title="Fetching now">
          {#each w.fetching.filter(seen) as a (`f:${a.release_group ?? a.album}`)}<ReleaseRow cover={coverOf(a)} title={a.album} line={watchLine(a)} fill={a.done} />{/each}
        </Group>
      {/if}
      <Group title="Not found yet" more="What Lidarr watches for and has not found yet (its Wanted).">
        {#each w.looking.filter(seen) as a (`l:${a.release_group ?? a.album}`)}<ReleaseRow cover={coverOf(a)} title={a.album} line={watchLine(a)} />{:else}{#if !w.looking.length}<Item name="Nothing" />{/if}{/each}
        {#if w.more}<Item name="{w.more.toLocaleString()} more" line="In Lidarr's Wanted" />{/if}
      </Group>
    {/if}
      <!-- On a phone, over the page: the global results at the end of this list (decision 0072). -->
    {#if handed && everywhere()}<GlobalResults />{/if}
  </div>
</div>

{#if selecting}
  <SelectBar label="Selected requests" count="{ticked.length}/{selectable.length}" onclose={() => (selecting = null)} every={{ all: !!selectable.length && ticked.length === selectable.length, onall: () => selectWhich('all'), onnone: () => selectWhich('none') }}>
    <button aria-label="Select which" title="Select…" onclick={() => (selectSheet = true)}><Icon name="listChecks" size={24} /></button>
    {#if list === 'mine'}
      <button aria-label="Take back" title="Take back" disabled={!ticked.length} onclick={takeBackTicked}><Icon name="undo" size={24} /></button>
    {:else}
      <button aria-label="Approve" title="Approve" disabled={!ticked.length} onclick={() => answer(ticked, true)}><Icon name="check" size={24} /></button>
      <button aria-label="Decline" title="Decline" disabled={!ticked.length} onclick={() => (declining = ticked)}><Icon name="ban" size={24} /></button>
    {/if}
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
        <MenuEntry icon="compass" label="Discover" onclick={() => ((opened = null), (location.hash = hrefOf('discover')))} />
      {/if}
    </div>
  </MenuSheet>
{/if}

{#if slskOpen}
  {@const i = soulseek.view?.items.find((x) => x.id === slskOpen?.id) ?? slskOpen}
  <!-- A Soulseek record (decision 0099): its folders to pick from (the best first), what was set aside and why, Search
       now, out of the list. -->
  <MenuSheet label={i.record.album} title={i.record.album} chips={[i.record.artist, ...(i.record.year ? [String(i.record.year)] : [])]} onclose={() => (slskOpen = null)}>
    {#if tickable(i)}
      <Group title="Folders" more="Each holds every track of the record, by title and length. The ticked record is fetched from the one chosen.">
        <ChoiceList
          label="Folder"
          options={i.folders.map((f, n) => ({ value: String(n), label: f.dir.split(/[\\/]/).pop() || f.dir, line: folderLine(f) }))}
          value={String(slskPicks[i.id] ?? i.chosen ?? 0)}
          onchange={(n) => (slskPicks = { ...slskPicks, [i.id]: Number(n) })}
        />
      </Group>
    {/if}
    {#if i.set_aside.length}
      <Group title="Set aside">
        {#each i.set_aside as a (a.user + a.path)}
          <Item lead="warn" name={a.path.split(/[\\/]/).pop() || a.path} line={[a.user, ...Object.entries(a.why).map(([f, w]) => `${f}: ${w}`)].join(' · ')} tone="warn" wrap />
        {/each}
      </Group>
    {/if}
    <div role="menu">
      {#if searchable(i)}<MenuEntry icon="search" label="Search now" onclick={() => (void searchSoulseekNow(i.id), (slskOpen = null))} />{/if}
      <MenuEntry icon="trash" label="Out of the list" danger onclick={() => (void dropSoulseek(i.id), (slskOpen = null))} />
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

  /* Its glass handed to App's row on a phone (lib/glass): the list starts under it, its first heading where the page's
     is (the glass's room, then its own 16 px; it began 12 px higher, the owner 2026-10-25). */
  .view.handed .scroll {
    padding-top: calc(var(--bar-room, 0px) + var(--mini-top, 0px) + var(--glass-top) + 16px);
    padding-bottom: calc(var(--mini-h, 0px) + var(--glass-bottom) + 16px);
  }
</style>
