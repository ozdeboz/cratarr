<script lang="ts">
  import ListHead from './lib/ListHead.svelte';
  import { PLAYLIST_ORDERS, PLAYLIST_WIDTHS, howMany, pageView, playlistBy, usePageAction } from './lib/pageView.svelte';
  import { passing, switchOn } from './lib/listFilter.svelte';
  import type { Playlist, PlayMode } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { live, queueTap } from './lib/live.svelte';
  import Notice from './lib/Notice.svelte';
  import { toast } from './lib/toast.svelte';
  import PlaylistMenu from './lib/PlaylistMenu.svelte';
  import SelectBar from './lib/SelectBar.svelte';
  import Icon from './lib/Icon.svelte';
  import Seg from './lib/Seg.svelte';
  import SizeControl from './lib/SizeControl.svelte';
  import { isLocal, setPlayingMember, shownPlaylists, startPlaylists } from './lib/output.svelte';
  import { pending, playing } from './lib/remote.svelte';
  import { longpress } from './lib/longpress';
  import { fromAction } from 'svelte/attachments';
  import PageLoading from './lib/PageLoading.svelte';
  import PlaylistItem from './lib/PlaylistItem.svelte';
  import { NEW_PLAYLIST, viewPlaylist, panel } from './lib/panel.svelte';
  import { isPlaylistFavorite, loadPlaylistFavorites, setPlaylistFavorites } from './lib/favourites.svelte';
  import { endSelect, pickWhich, viewPicked, select, showsPicked, startSelect, tick } from './lib/select.svelte';
  import SelectSheet from './lib/SelectSheet.svelte';
  import Row, { type Cell } from './lib/Row.svelte';
  import { fit } from './lib/columns.svelte';
  import { playlistStats, type PlaylistStats } from './lib/api';
  import { addedOn } from './lib/albums.svelte';
  import { clockText } from './lib/remote.svelte';

  // What is playing and the playlists come from the shell's poll (live.svelte.ts).
  const view = $derived(live.view);
  // The favourites' hearts on the chips (decision 0023).
  $effect(() => void loadPlaylistFavorites());
  const error = $derived(live.error);
  /** The filter section's Favourites toggle: only favourite playlists. */
  // Size = chip width, which sets how many columns fit; one width per step in prefs.ZOOM_STEPS.
  let gridWidth = $state(0);
  $effect(() => {
    pageView.width = gridWidth;
  });
  // Taps flip the chip at once; the server's answer replaces this guess.
  let optimistic = $state<Record<string, boolean>>({});
  let busy = $state<Record<string, boolean>>({});

  // The open menu (a sheet, decision 0021): a playlist's (lib/PlaylistMenu), the "New playlist" one, or the ticked
  // playlists' (Select's ⋮, decision 0025).
  type Menu = { kind: 'selection' } | { kind: 'one'; id: string };
  let menu = $state<Menu | null>(null);

  const np = $derived(playing());
  // Its conditions (decision 0061): the server says which playlists pass, any or all of their tracks.
  const pass = passing(() => 'playlists', () => ({ list: { kind: 'playlists' }, source: prefs.source || undefined }));
  const items = $derived.by(() => {
    const q = pageView.query.trim().toLowerCase();
    let list = shownPlaylists().map((p) => {
      const member = optimistic[p.id] ?? p.member;
      return { ...p, member, count: p.count + (member === p.member ? 0 : member ? 1 : -1) };
    });
    if (q) list = list.filter((p) => p.name.toLowerCase().includes(q));
    if (switchOn('playlists', 'favourites')) list = list.filter((p) => isPlaylistFavorite(p.id));
    if (switchOn('playlists', 'mine')) list = list.filter((p) => p.mine !== false);
    const keys = pass.keys;
    if (keys) list = list.filter((p) => keys.has(p.id));
    // Pinned ones first, in the order pinned; the rest in the list's order (its header, or View), either way round.
    const pin = (id: string) => {
      const i = prefs.pinned.indexOf(id);
      return i < 0 ? Infinity : i;
    };
    const by = playlistBy<Playlist>(prefs.playlistOrder, (p) => p.member);
    const turn = prefs.playlistDown ? -1 : 1;
    return [...list].sort((a, b) => {
      const [pa, pb] = [pin(a.id), pin(b.id)];
      if ((pa === Infinity) !== (pb === Infinity)) return pa === Infinity ? 1 : -1;
      return pa === Infinity ? turn * by(a, b) : pa - pb;
    });
  });

  // How many the filter lets through, of how many (the Filter sheet's count).
  $effect(() => {
    pageView.counted = { shown: items.length, total: shownPlaylists().length };
    return () => (pageView.counted = null);
  });

  const memberCount = $derived(shownPlaylists().filter((p) => optimistic[p.id] ?? p.member).length);
  const NOT_MUSIC = "Not in your Music, so it can't go in playlists.";

  async function tap(p: Playlist) {
    if (busy[p.id]) return;
    if (!np) {
      toast.show({ text: 'Nothing is playing. Start a track to add it to playlists.', tone: 'warn' });
      return;
    }
    if (!np.addable && !p.member) {
      toast.show({ text: NOT_MUSIC, tone: 'warn' });
      return;
    }
    // The server's player is moving to another track: the tap waits for it (live.svelte.ts). The local track's
    // playlists are a moment away: the tap is not taken.
    if (pending()) {
      if (!isLocal()) queueTap(p.id);
      return;
    }
    busy[p.id] = true;
    optimistic[p.id] = !p.member;
    try {
      await setPlayingMember(p.id, !p.member);
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      delete optimistic[p.id];
      delete busy[p.id];
    }
  }

  const missing = (n: number) => `${n} missing ${n === 1 ? 'track' : 'tracks'}`;

  // The table (decision 0042): a playlist a line, its facts in columns, as many as the page's width allows in the
  // owner's order (View). The facts are asked for while the table shows, again when the playlists change.
  const table = $derived(prefs.playlistLayout === 'table');
  const cols = $derived(table ? fit('playlists', gridWidth) : []);
  let stats = $state<Record<string, PlaylistStats>>({});
  const listsKey = $derived(shownPlaylists().map((p) => `${p.id}:${p.count}`).join('|'));
  $effect(() => {
    if (!table || !cols.length) return;
    void listsKey;
    playlistStats()
      .then((s) => (stats = s))
      .catch(() => {});
  });
  function cellText(id: string, st: PlaylistStats | undefined): string {
    if (!st) return '';
    if (id === 'length') return st.seconds ? clockText(st.seconds) : '';
    if (id === 'genres') return st.genres.join(', ');
    if (id === 'lastPlayed') return st.last_played ? addedOn(st.last_played) : '';
    if (id === 'changed') return st.changed ? addedOn(st.changed) : '';
    if (id === 'unplayed') return st.unplayed ? st.unplayed.toLocaleString() : '';
    return '';
  }
  const cellsOf = (id: string): Cell[] | undefined => (cols.length ? cols.map((c) => ({ ...c, text: cellText(c.id, stats[id]) })) : undefined);

  // The pill's last button (lib/TopPill): New playlist, the panel's edit mode with no playlist yet. What is ticked has
  // the select bar.
  usePageAction(() => ({ icon: 'plus', label: 'New playlist', run: () => viewPlaylist(NEW_PLAYLIST) }));

  // The select bar's actions on the ticked playlists (one after another, each track once), as the panel's on its
  // ticked tracks: Select ends after each.
  const ticked = $derived(select.picked.filter((id) => shownPlaylists().some((x) => x.id === id)));
  // In Select a long-press asks what to tick: all, none or the others.
  let selectSheet = $state(false);
  const tickedFav = $derived(ticked.length > 0 && ticked.every((id) => isPlaylistFavorite(id)));
  async function playTicked(mode: PlayMode) {
    const ids = [...ticked];
    if (!ids.length) return;
    navigator.vibrate?.(8);
    endSelect();
    try {
      await startPlaylists(ids, mode);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
  function favouriteTicked() {
    navigator.vibrate?.(8);
    setPlaylistFavorites([...ticked], !tickedFav);
    endSelect();
  }

</script>

<main>
  <!-- The search, size, favourites-only and New playlist are the pill's (lib/TopPill), the order the list's header,
       what is ticked the select bar's (decided 2026-10-15/16). -->
  {#if error}
    <Notice text={error} />
  {/if}

  {#if !view && !error}
    <PageLoading />
  {:else}
    <!-- The list's header (lib/ListHead): its order, how many, and Select (decision 0025: on, a chip's name ticks it; the
         ticked ones are the select bar's: played, favoured, viewed in the panel, their menu). -->
    <ListHead
      fields={PLAYLIST_ORDERS}
      bind:value={prefs.playlistOrder}
      bind:down={prefs.playlistDown}
      count={howMany(items.length, 'playlist')}
      selecting={select.on}
      onselect={() => (select.on ? endSelect() : startSelect())}
    />
    {#if table && cols.length}
      <!-- The table's column names, a row of the same shape so they line up. -->
      <div class="table-head">
        <Row size="large" as="div" head title="Name" count="Tracks" cells={cols.map((c) => ({ ...c, text: c.label }))}>
          {#snippet action()}<span></span>{/snippet}
        </Row>
      </div>
    {/if}
    <ul class="grid" class:table class:pending={pending()} style:--min="{PLAYLIST_WIDTHS[prefs.zoom]}px" bind:clientWidth={gridWidth}>
      {#each items as p (p.id)}
        <li>
          <!-- The label views the playlist (decision 0023), in Select ticks it (decision 0025); the circle adds or removes
               the playing track. The border: in the panel's selection (or ticked). -->
          <PlaylistItem
            {p}
            size={table ? 'large' : 'medium'}
            cells={table ? cellsOf(p.id) : undefined}
            class={['chip', busy[p.id] && 'busy', live.queued.includes(p.id) && 'queued', !np && 'idle', !!np && !np.addable && !p.member && 'blocked']
              .filter(Boolean)
              .join(' ')}
            strong={p.member}
            active={select.on ? select.picked.includes(p.id) : panel.selection.includes(p.id)}
            label={select.on ? `${select.picked.includes(p.id) ? 'Untick' : 'Tick'} ${p.name}` : `View ${p.name}`}
            onclick={() => (select.on ? tick(p.id) : viewPlaylist(p.id))}
            attach={fromAction(longpress, () => () => {
              // In Select it asks what to tick (lib/SelectSheet); else the playlist's menu (decision 0058).
              if (select.on) return void (selectSheet = true);
              menu = { kind: 'one', id: p.id };
            })}
          >
            {#snippet action()}
              <button
                class="icon"
                aria-pressed={p.member}
                aria-label={p.member ? `Take the playing track out of ${p.name}` : `Add the playing track to ${p.name}`}
                onclick={() => tap(p)}
                ><Icon name={p.member ? 'circleCheck' : np && !np.addable ? 'circleDashedPlus' : 'circlePlus'} size={26} /></button
              >
            {/snippet}
          </PlaylistItem>
        </li>
      {/each}
    </ul>
    {#if view && items.length === 0}<p class="empty">No playlists match.</p>{/if}
  {/if}
  {#if select.on}
    <!-- The panel's select bar (lib/SelectBar): play, put next, queue or favour the ticked playlists, view them in the
         panel (unless it shows them), their menu. -->
    {@const viewLabel = ticked.length === 1 ? 'View 1 playlist' : `View ${ticked.length} playlists`}
    <SelectBar label="Selected playlists" count="{ticked.length}/{items.length}" onclose={endSelect} place="page">
      <button aria-label="Play" title="Play" disabled={!ticked.length} onclick={() => playTicked('play')}><Icon name="play" size={24} /></button>
      <button class:fav={tickedFav} aria-pressed={tickedFav} aria-label="Favourite" title="Favourite" disabled={!ticked.length} onclick={favouriteTicked}
        ><Icon name="heart" size={24} /></button
      >
      {#if !showsPicked()}
        <button aria-label={viewLabel} title={viewLabel} disabled={!ticked.length} onclick={viewPicked}><Icon name="squareArrowOutUpRight" size={24} /></button>
      {/if}
      <button aria-label="Menu of the ticked playlists" title="More" disabled={!ticked.length} onclick={() => (menu = { kind: 'selection' })}
        ><Icon name="moreVert" size={24} /></button
      >
    </SelectBar>
  {/if}
</main>

{#if menu?.kind === 'selection'}
  <!-- The menu of what is ticked. -->
  <PlaylistMenu playlists={select.picked.flatMap((id) => shownPlaylists().filter((x) => x.id === id))} onclose={() => (menu = null)} />
{:else if menu?.kind === 'one'}
  <!-- A playlist's own menu (its long-press). -->
  {@const id = menu.id}
  <PlaylistMenu playlists={shownPlaylists().filter((x) => x.id === id)} onclose={() => (menu = null)} />
{/if}

{#if selectSheet}
  <SelectSheet
    onall={() => pickWhich(items.map((p) => p.id), 'all')}
    onnone={() => pickWhich(items.map((p) => p.id), 'none')}
    onothers={() => pickWhich(items.map((p) => p.id), 'others')}
    onclose={() => (selectSheet = false)}
  />
{/if}

<style>
  /* The page answers to its own width (beside the player or not), not the window's: a container. */
  main {
    position: relative;
    padding: 14px clamp(12px, 2vw, 32px) 48px;
    container-type: inline-size;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--min, 170px), 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* A table (decision 0042): a playlist a line, the lines touching, lower than a track's (no cover). */
  .grid.table {
    display: block;
  }

  .grid.table :global(.row.large),
  .table-head :global(.row.large) {
    min-height: 48px;
  }

  .table-head :global(.row.head) {
    min-height: 32px;
  }

  /* The chips are medium rows (lib/Row.svelte, Finamp's add-to-playlist tile); what is the page's own is how adding
     the playing track stands. Nothing playing: the chips only view, so they look quieter. */
  .grid :global(.chip.idle) {
    opacity: 0.6;
  }

  .grid :global(.chip.busy) {
    opacity: 0.7;
  }

  /* The player is moving to another track: its playlists are not known yet, so the chips are dimmed, and a tapped one
     pulses until the tap goes through (live.svelte.ts). */
  .grid.pending :global(.chip:not(.queued)) {
    opacity: 0.45;
  }

  .grid :global(.chip.queued) {
    outline: 1.5px solid var(--accent);
    animation: waiting 1s ease-in-out infinite alternate;
  }

  @keyframes waiting {
    from {
      opacity: 1;
    }
    to {
      opacity: 0.55;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .grid :global(.chip.queued) {
      animation: none;
    }
  }

  /* Track outside Music: adding is refused, so the add chips are quieter and their plus circle dashed (Finamp's mark
     for a playlist it cannot add to); tapping explains why. */
  .grid :global(.chip.blocked) {
    opacity: 0.6;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }

</style>
