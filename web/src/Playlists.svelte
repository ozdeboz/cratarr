<script lang="ts">
  import ListHead from './lib/ListHead.svelte';
  import { PLAYLIST_ORDERS, PLAYLIST_WIDTHS, howMany, pageView, usePageAction } from './lib/pageView.svelte';
  import { createPlaylist, type Playlist, type PlayMode } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { live, queueTap, refresh } from './lib/live.svelte';
  import Notice from './lib/Notice.svelte';
  import { toast } from './lib/toast.svelte';
  import { withUndo } from './lib/undo.svelte';
  import { followCreate, followDelete } from './lib/follow.svelte';
  import PlaylistMenu from './lib/PlaylistMenu.svelte';
  import SelectBar from './lib/SelectBar.svelte';
  import NameSheet from './lib/NameSheet.svelte';
  import Icon from './lib/Icon.svelte';
  import Seg from './lib/Seg.svelte';
  import SizeControl from './lib/SizeControl.svelte';
  import { isLocal, setPlayingMember, shownPlaylists, startPlaylists } from './lib/output.svelte';
  import { pending, playing } from './lib/remote.svelte';
  import { longpress } from './lib/longpress';
  import { fromAction } from 'svelte/attachments';
  import PageLoading from './lib/PageLoading.svelte';
  import PlaylistItem from './lib/PlaylistItem.svelte';
  import { viewPlaylist, panel } from './lib/panel.svelte';
  import { isPlaylistFavorite, loadPlaylistFavorites, setPlaylistFavorites } from './lib/favourites.svelte';
  import { endSelect, viewPicked, select, showsPicked, startSelect, tick } from './lib/select.svelte';

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
  $effect(() => () => (pageView.favouritesOnly = false));
  // Taps flip the chip at once; the server's answer replaces this guess.
  let optimistic = $state<Record<string, boolean>>({});
  let busy = $state<Record<string, boolean>>({});

  // The open menu (a sheet, decision 0021): a playlist's (lib/PlaylistMenu), the "New playlist" one, or the ticked
  // playlists' (Select's ⋮, decision 0025).
  type Menu = { kind: 'new' } | { kind: 'selection' };
  let menu = $state<Menu | null>(null);
  let working = $state(false);

  const np = $derived(playing());
  const items = $derived.by(() => {
    const q = pageView.query.trim().toLowerCase();
    let list = shownPlaylists().map((p) => {
      const member = optimistic[p.id] ?? p.member;
      return { ...p, member, count: p.count + (member === p.member ? 0 : member ? 1 : -1) };
    });
    if (q) list = list.filter((p) => p.name.toLowerCase().includes(q));
    if (pageView.favouritesOnly) list = list.filter((p) => isPlaylistFavorite(p.id));
    // Pinned ones first, in the order pinned; the rest in the list's order (its header, or View), either way round.
    const pin = (id: string) => {
      const i = prefs.pinned.indexOf(id);
      return i < 0 ? Infinity : i;
    };
    const byName = (a: Playlist, b: Playlist) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true });
    const by = (a: Playlist & { member: boolean }, b: Playlist & { member: boolean }) =>
      prefs.playlistOrder === 'tracks'
        ? a.count - b.count || byName(a, b)
        : prefs.playlistOrder === 'playing'
          ? Number(b.member) - Number(a.member) || byName(a, b)
          : byName(a, b);
    const turn = prefs.playlistDown ? -1 : 1;
    return [...list].sort((a, b) => {
      const [pa, pb] = [pin(a.id), pin(b.id)];
      if ((pa === Infinity) !== (pb === Infinity)) return pa === Infinity ? 1 : -1;
      return pa === Infinity ? turn * by(a, b) : pa - pb;
    });
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

  async function create(name: string) {
    if (working) return;
    working = true;
    try {
      const { undo } = await createPlaylist(name);
      withUndo(`Created ${name}.`, [undo], { after: (undone) => (undone ? followDelete(name) : followCreate(name)) });
      menu = null;
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      working = false;
      await refresh();
    }
  }

  // The pill's last button (lib/TopPill): New playlist. What is ticked has the select bar.
  usePageAction(() => ({ icon: 'plus', label: 'New playlist', run: () => (menu = { kind: 'new' }) }));

  // The select bar's actions on the ticked playlists (one after another, each track once), as the panel's on its
  // ticked tracks: Select ends after each.
  const ticked = $derived(select.picked.filter((id) => shownPlaylists().some((x) => x.id === id)));
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
    <ul class="grid" class:pending={pending()} style:--min="{PLAYLIST_WIDTHS[prefs.zoom]}px" bind:clientWidth={gridWidth}>
      {#each items as p (p.id)}
        <li>
          <!-- The label views the playlist (decision 0023), in Select ticks it (decision 0025); the circle adds or removes
               the playing track. The border: in the panel's selection (or ticked). -->
          <PlaylistItem
            {p}
            size="medium"
            class={['chip', busy[p.id] && 'busy', live.queued.includes(p.id) && 'queued', !np && 'idle', !!np && !np.addable && !p.member && 'blocked']
              .filter(Boolean)
              .join(' ')}
            strong={p.member}
            active={select.on ? select.picked.includes(p.id) : panel.selection.includes(p.id)}
            label={select.on ? `${select.picked.includes(p.id) ? 'Untick' : 'Tick'} ${p.name}` : `View ${p.name}`}
            onclick={() => (select.on ? tick(p.id) : viewPlaylist(p.id))}
            attach={fromAction(longpress, () => () => {
              if (!select.on) startSelect();
              if (!select.picked.includes(p.id)) tick(p.id);
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
      <button aria-label="Play next" title="Play next" disabled={!ticked.length} onclick={() => playTicked('next')}><Icon name="playNext" size={24} /></button>
      <button aria-label="Add to queue" title="Add to queue" disabled={!ticked.length} onclick={() => playTicked('last')}><Icon name="listAdd" size={24} /></button>
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

{#if menu?.kind === 'new'}
  <NameSheet title="New playlist" what="playlist" action="Create playlist" onsubmit={create} onclose={() => (menu = null)} />
{:else if menu?.kind === 'selection'}
  <!-- The menu of what is ticked (a long-press ticks). -->
  <PlaylistMenu playlists={select.picked.flatMap((id) => shownPlaylists().filter((x) => x.id === id))} onclose={() => (menu = null)} />
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
