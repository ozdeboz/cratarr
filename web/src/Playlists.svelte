<script lang="ts">
  import ListHead from './lib/ListHead.svelte';
  import { PLAYLIST_ORDERS, PLAYLIST_WIDTHS, howMany, pageView, usePageAction } from './lib/pageView.svelte';
  import { createPlaylist, type Playlist } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { live, queueTap, refresh } from './lib/live.svelte';
  import Notice from './lib/Notice.svelte';
  import { toast } from './lib/toast.svelte';
  import { withUndo } from './lib/undo.svelte';
  import { followCreate, followDelete } from './lib/follow.svelte';
  import PlaylistMenu from './lib/PlaylistMenu.svelte';
  import NameSheet from './lib/NameSheet.svelte';
  import Icon from './lib/Icon.svelte';
  import Seg from './lib/Seg.svelte';
  import SizeControl from './lib/SizeControl.svelte';
  import { isLocal, setPlayingMember, shownPlaylists } from './lib/output.svelte';
  import { pending, playing } from './lib/remote.svelte';
  import { longpress } from './lib/longpress';
  import { fromAction } from 'svelte/attachments';
  import PlaylistItem from './lib/PlaylistItem.svelte';
  import { viewPlaylist, panel } from './lib/panel.svelte';
  import { isPlaylistFavorite, loadPlaylistFavorites } from './lib/favourites.svelte';
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

  // The pill's last button (lib/TopPill): New playlist, or with playlists ticked their menu (⋮, decision 0025).
  usePageAction(() =>
    select.on && select.picked.length
      ? { icon: 'moreVert', label: 'Menu of the ticked playlists', run: () => (menu = { kind: 'selection' }) }
      : { icon: 'plus', label: 'New playlist', run: () => (menu = { kind: 'new' }) },
  );

</script>

<main>
  <!-- View N while ticking (the ticked ones in the panel). The search, size, favourites-only, New playlist and the
       ticked ones' menu are the pill's (lib/TopPill), the order the list's header (decided 2026-10-15). -->
  {#if select.on && select.picked.length && !showsPicked()}
    {@const viewLabel = select.picked.length === 1 ? 'View 1 playlist' : `View ${select.picked.length} playlists`}
    <div class="tools">
      <button class="new primary view-picked" aria-label={viewLabel} title={viewLabel} onclick={viewPicked}><Icon name="playlists" size={22} /></button>
    </div>
  {/if}
  {#if error}
    <Notice text={error} />
  {/if}

  {#if !view && !error}
    <p class="empty">Loading…</p>
  {:else}
    <!-- The list's header (lib/ListHead): its order, how many, and Select (decision 0025: on, a chip's name ticks it; the
         ticked ones' menu is the ⋮ at the pill's end; on a phone they are viewed in the panel with View N). -->
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

  .tools {
    display: flex;
    margin: 0 0 14px;
  }

  button.new {
    min-height: 44px;
    padding: 0 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    cursor: pointer;
  }

  .view-picked {
    display: grid;
    place-items: center;
    width: 44px;
    margin-left: auto;
    padding: 0;
  }

  button.new.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  button.new:disabled {
    opacity: 0.5;
    cursor: default;
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
