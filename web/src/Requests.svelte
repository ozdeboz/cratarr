<script lang="ts">
  // Requests (decision 0070; the owner, 2026-10-24: a queue "the way Seerr works", "Requests, left of Downloads"):
  // albums asked for in Discover, each with its request's status and its album's own (being fetched, in Incoming, in the
  // library). Laid out as Settings, Health and Downloads: groups of rows. For those who answer requests, everyone's
  // waiting ones first, answered one at a time from a row's menu or many at once (the select mode every list has: the
  // group's icon starts it, the bar acts on the ticked ones). Then the account's own, a waiting one taken back from its
  // menu. Then what Lidarr is looking for and fetching now (its Wanted and queue), read while the page shows.
  import {
    cancelRequest,
    decideRequests,
    getRequests,
    getWatching,
    type AlbumRequest,
    type RequestsView,
    type WatchedAlbum,
    type Watching,
  } from './lib/api';
  import Icon from './lib/Icon.svelte';
  import MenuEntry from './lib/MenuEntry.svelte';
  import MenuSheet from './lib/MenuSheet.svelte';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import SelectBar from './lib/SelectBar.svelte';
  import SelectSheet from './lib/SelectSheet.svelte';
  import Group from './lib/settings/Group.svelte';
  import { me } from './lib/me.svelte';
  import Item from './lib/settings/Item.svelte';
  import ValueSheet from './lib/settings/ValueSheet.svelte';
  import { when } from './lib/settings/when';
  import { toast } from './lib/toast.svelte';

  let view = $state<RequestsView | null>(null);
  let watching = $state<Watching | null>(null);
  let error = $state<string | null>(null);
  const load = () =>
    getRequests().then(
      (v) => ((view = v), (error = null)),
      (e) => (error = e instanceof Error ? e.message : String(e)),
    );
  // Lidarr asked, not watched: read with the page, and every half minute while it shows (its queue moves).
  const look = () => getWatching().then((w) => (watching = w)).catch(() => {});
  load();
  $effect(() => {
    look();
    const t = setInterval(look, 30_000);
    return () => clearInterval(t);
  });

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

  // --- A row's menu ---
  let opened = $state<AlbumRequest | null>(null);
  const answers = $derived(!!view?.may.manage_requests);
  const canTakeBack = (r: AlbumRequest) => (r.status === 'pending' && (r.user === me.name || answers)) || (r.status === 'approved' && answers);

  // --- Answering ---
  let declining = $state<number[] | null>(null);
  async function decide(ids: number[], approve: boolean, reason: string | null = null) {
    try {
      const out = await decideRequests(ids, approve, reason);
      // Only what went wrong is said (the rows show the rest).
      const wrong = out.filter((o) => o.said.startsWith('Not handed over') || o.said.startsWith('Already') || o.said.includes('gone'));
      if (wrong.length) toast.show({ text: wrong.length > 1 ? `${wrong[0].said} (and ${wrong.length - 1} more)` : wrong[0].said, tone: 'error' });
    } catch (e) {
      toast.fail(e, approve ? 'Could not approve it' : 'Could not decline it');
    }
    selecting = null;
    load();
    look();
  }
  async function takeBack(r: AlbumRequest) {
    try {
      await cancelRequest(r.id);
    } catch (e) {
      toast.fail(e, 'Could not take it back');
    }
    load();
  }

  // --- Select (everyone's waiting ones) ---
  let selecting = $state<Set<number> | null>(null);
  let selectSheet = $state(false);
  const waiting = $derived(view?.waiting ?? []);
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

  const watchLine = (w: WatchedAlbum) =>
    [w.artist, w.year, w.done !== null ? `${Math.round(w.done * 100)}%${w.status && w.status !== 'downloading' ? ` · ${w.status}` : ''}` : null]
      .filter(Boolean)
      .join(' · ');
</script>

{#snippet row(r: AlbumRequest, who: boolean, tickable: boolean)}
  {#if tickable && selecting}
    <Item name={r.album} line={line(r, who)} tone={tone(r)}>
      <button class="tick" class:on={selecting.has(r.id)} role="checkbox" aria-checked={selecting.has(r.id)} aria-label={`Select ${r.album}`} onclick={() => tick(r.id)}
        ><Icon name={selecting.has(r.id) ? 'circleCheck' : 'circle'} size={24} /></button
      >
    </Item>
  {:else}
    <Item name={r.album} line={line(r, who)} tone={tone(r)} progress={fill(r)} onclick={() => (opened = r)} />
  {/if}
{/snippet}

<main>
  {#if error && !view}
    <Notice text={error} />
  {:else if !view}
    <PageLoading />
  {:else}
    {#if answers && !view.fetcher}
      <p class="note">Approved requests need Lidarr: <a href="#/settings?view=settings%3Aconnections%2Flidarr">connect it</a>.</p>
    {/if}
    {#if waiting.length}
      <Group title="Waiting for an answer">
        {#snippet tail()}
          <button class="head-icon" aria-label="Select" title="Select" aria-pressed={!!selecting} onclick={() => (selecting = selecting ? null : new Set())}
            ><Icon name="listChecks" size={20} /></button
          >
        {/snippet}
        {#each waiting as r (r.id)}{@render row(r, true, true)}{/each}
      </Group>
    {/if}
    <Group title="Your requests">
      {#each view.mine as r (r.id)}{@render row(r, false, false)}{:else}
        <Item name="None yet" line="Want an album in Discover to ask for it." onclick={() => (location.hash = '#/discover')} />
      {/each}
    </Group>
    {#if watching && (watching.fetching.length || watching.looking.length)}
      <Group title="Lidarr is looking for these" more="What Lidarr is fetching now, then what it watches for and has not found yet.">
        {#each watching.fetching as w (`f:${w.release_group ?? w.album}`)}<Item name={w.album} line={watchLine(w)} progress={w.done} />{/each}
        {#each watching.looking as w (`l:${w.release_group ?? w.album}`)}<Item name={w.album} line={watchLine(w)} />{/each}
        {#if watching.more}<Item name="{watching.more.toLocaleString()} more" line="In Lidarr's Wanted" />{/if}
      </Group>
    {/if}
  {/if}
</main>

{#if selecting}
  <SelectBar label="Selected requests" count="{ticked.length}/{waiting.length}" onclose={() => (selecting = null)} place="page">
    <button aria-label="Select which" title="Select…" onclick={() => (selectSheet = true)}><Icon name="listChecks" size={24} /></button>
    <button aria-label="Approve" title="Approve" disabled={!ticked.length} onclick={() => decide(ticked, true)}><Icon name="check" size={24} /></button>
    <button aria-label="Decline" title="Decline" disabled={!ticked.length} onclick={() => (declining = ticked)}><Icon name="ban" size={24} /></button>
  </SelectBar>
{/if}
{#if selectSheet}
  <SelectSheet onall={() => selectWhich('all')} onnone={() => selectWhich('none')} onothers={() => selectWhich('others')} onclose={() => (selectSheet = false)} />
{/if}

{#if opened}
  {@const r = opened}
  <MenuSheet label={r.album} title={r.album} images={r.cover ? [r.cover] : []} chips={[r.artist, ...(r.year ? [String(r.year)] : [])]} onclose={() => (opened = null)}>
    <div role="menu">
      {#if answers && (r.status === 'pending' || (r.status === 'approved' && r.fetched?.startsWith('Not handed over')))}
        <MenuEntry icon="check" label={r.status === 'pending' ? 'Approve' : 'Hand over again'} onclick={() => ((opened = null), decide([r.id], true))} />
      {/if}
      {#if answers && r.status === 'pending'}
        <MenuEntry icon="ban" label="Decline…" onclick={() => ((opened = null), (declining = [r.id]))} />
      {/if}
      {#if canTakeBack(r)}
        <MenuEntry icon="undo" label="Take back" danger onclick={() => ((opened = null), takeBack(r))} />
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
    onsave={(why: string) => ((declining = null), decide(ids, false, why.trim() || null))}
    onclose={() => (declining = null)}
  />
{/if}

<style>
  /* As Settings' and Health's list: one column of groups. */
  main {
    display: grid;
    gap: 24px;
    max-width: 640px;
    padding: 16px clamp(12px, 2vw, 32px) 96px;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 14px;
  }

  .note a {
    color: var(--accent-ink);
  }

  .head-icon,
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

  .head-icon[aria-pressed='true'],
  .tick.on {
    color: var(--accent);
  }
</style>
