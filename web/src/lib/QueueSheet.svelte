<script lang="ts">
  import StartStopSheet from './StartStopSheet.svelte';
  import { leavePlayer } from './navStep.svelte';
  import { port } from './player/port.svelte';
  // The queue (decisions 0019, 0022), as Finamp's queue sheet (lib/components/PlayerScreen/queue_list.dart): a sheet
  // over the player on the blurred cover, a grab handle and "Queue"; then the previous tracks (folded, this device
  // only: the server's player reports only what comes next), the playing track as a card that fills as it plays
  // (pinned at the top), the "Playing from" header (pinned under it) and what comes next. From Symfonium's queue
  // (decided 2026-10-16): search (in place, the row grown into a field) and Clear (what comes next goes, with Undo) in
  // the head, shuffle and repeat being the player's; the queues kept, to switch to one (decision 0033, amended:
  // lib/QueuesSheet) from the head too; More music (Radio and Autoplay) a pill floating where the queue ends;
  // when the queue ends on the clock; "Playing from" a chip that opens it; a row's album after its artist (not the
  // album the queue plays), and its ⋮.
  // Gestures as Finamp's: drag the handle area down to close; a tap on a row plays it, a long-press (or right-click) is
  // its menu; a row's heart adds it to a playlist, a long-press on the heart makes it a favourite or not; on this device
  // a row's handle reorders and a swipe left removes it (the server's player takes neither command). Haptics as
  // Finamp's: light on a tap or a drag starting, heavy on a drop, a removal and opening.
  import { matches as matchesWords, words } from './search';
  import { untrack } from 'svelte';
  import { linear } from 'svelte/easing';
  import { fade, fly } from 'svelte/transition';
  import { albumCoverUrl } from './api';
  import Cover, { coverBroke, coverBroken } from './Cover.svelte';
  import { loadFavorites } from './favourites.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import RowHeart from './RowHeart.svelte';
  import { swipe } from './swipe';
  import SwipeBehind from './SwipeBehind.svelte';
  import { sheetDrag } from './sheetDrag';
  import QueuePlaylistSheet from './QueuePlaylistSheet.svelte';
  import SubjectMenu from './SubjectMenu.svelte';
  import QueueMenu from './QueueMenu.svelte';
  import QueuesSheet from './QueuesSheet.svelte';
  import { Reorder } from './reorder.svelte';
  import TrackMenu, { type MenuTrack, type QueuePlace } from './TrackMenu.svelte';
  import { clearUpcoming, local, jump as localJump, moveUpcoming, removeUpcoming, restoreUpcoming } from './localPlayer.svelte';
  import MoreMusicSheet, { moreMusicLine, moreMusicParts } from './MoreMusicSheet.svelte';
  import { canPreview, endPreview, preview, startPreview } from './preview.svelte';
  import { hold } from './hold';
  import { albumSubject, viewPlaylist } from './panel.svelte';
  import { CRATE, crates, loadCrates, queueCrate } from './crates.svelte';
  import HeartSheet from './HeartSheet.svelte';
  import { BANDCAMP } from './bandcampPreview';
  import { player } from './live.svelte';
  import { decelerate, reducedMotion, SHEET_IN, SHEET_OUT } from './motion';
  import { shownPlaylists } from './output.svelte';
  import { clockText, coverSrc, duration, favorite, paused, pending, playUpcoming, playing, position, send, toggleFavorite, playPause, rampInert, shownPaused } from './remote.svelte';
  import { toast } from './toast.svelte';
  import { accept, askedOf, askerName, decline, follow, followingNow, loadAsked, playerShown, requestsOffered, waiting } from './following.svelte';
  import { mine } from './mySettings.svelte';
  import { prefs } from './prefs.svelte';
  import { ACCEPTED_MS, requestsShown, stillComing, withRequests } from './queueRequests';

  /** `screen`: over the whole window, as the other sheets (opened from the mini-player, the owner 2026-10-25: it was
   *  held inside the bar); else over the full player that holds it. */
  let { src, onclose, screen = false }: { src: string | null; onclose: () => void; screen?: boolean } = $props();

  const np = $derived(playing());
  const pos = $derived(position());
  const dur = $derived(duration());
  const held = $derived(port().queue());
  const here = $derived(held !== null);
  // Finamp's haptics: a selection tick and a heavy one.
  const tick = () => navigator.vibrate?.(8);
  const heavy = () => navigator.vibrate?.(25);
  heavy();

  // The previous tracks: folded, as Finamp's default.
  let prevOpen = $state(false);
  const previous = $derived(held ? held.tracks.slice(0, Math.max(0, held.index)) : []);

  // The queue section's header: where the queue came from, and "i / n (remaining)".
  const from = $derived(held?.from ?? null);
  const count = $derived.by(() => {
    if (!np) return '';
    const n = held ? held.tracks.length : np.upcoming.length + 1;
    const i = held ? held.index + 1 : 1;
    const lengths = np.upcoming.map((u) => u.duration_secs);
    const left = dur && lengths.every((l) => l !== null) ? dur - pos + lengths.reduce((a: number, l) => a + (l ?? 0), 0) : null;
    if (left === null) return `${i} of ${n}`;
    const ends = new Date(Date.now() + left * 1000).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
    return `${i} of ${n} · ${clockText(left)} · until ${ends}`;
  });

  // "Playing from" opens what the queue came from, in the panel: its album when the queue's own tracks (not those
  // added by hand or by the radio) are one album's, else the playlist or crate of that name.
  const fromView = $derived.by(() => {
    if (!from) return null;
    const own = local.queue.filter((t) => !t.lane);
    const first = own[0];
    if (first?.album_id && own.every((t) => t.album_id === first.album_id)) return albumSubject(first.source, first.album_id);
    if (shownPlaylists().some((p) => p.id === from)) return from;
    const c = queueCrate(from);
    return c ? CRATE + c : null;
  });
  /** A crate's chip needs the crates, which nothing else may have read: read here, and until then the line says
   *  nothing rather than "Playing from" as words that turn into a chip (the owner, 2026-10-24). */
  let cratesComing = $state(false);
  if (port().queue()?.from && crates.list === null) {
    cratesComing = true;
    void loadCrates().finally(() => (cratesComing = false));
  }
  /** The plays-from chip's menu (decision 0058: a badge that opens an object opens its menu on a long-press). */
  let fromMenu = $state<string | null>(null);
  function openFrom() {
    const v = fromView;
    if (!v) return;
    onclose();
    leavePlayer();
    viewPlaylist(v);
  }

  // Search: what comes next, by title, artist or album; the rows that match show (no reordering meanwhile).
  // More music's sheet (Radio and Autoplay; decision 0036, amended), from the floating pill.
  let moreOpen = $state(false);

  let searching = $state(false);
  /** New playlist from the queue, its name sheet open. */
  let saving = $state(false);
  /** The queues kept (decision 0033, amended), to switch to one: its sheet open. */
  let switching = $state(false);
  let query = $state('');
  // As every search reads (lib/search, decision 0072).
  const q = $derived(searching ? words(query) : []);
  const matches = (t: { name: string; artist: string | null; album: string | null }) => matchesWords(q, t.name, t.artist, t.album);


  // Clear: what comes next goes; the toast's Undo puts it back.
  /** The queue's menu (lib/QueueMenu). */
  let queueMenu = $state(false);
  function clear() {
    heavy();
    const removed = clearUpcoming();
    if (removed.length) toast.show({ text: 'Queue cleared.', tone: 'done', act: { label: 'Undo', run: () => restoreUpcoming(removed) } });
  }

  let list = $state<HTMLElement>();

  // The rows' hearts: which of the listed tracks are favourites, asked for in one go when the list changes.
  const listed = $derived(np ? [...previous.map((t) => t.id), ...np.upcoming.map((u) => u.id)] : []);
  const listedKey = $derived(listed.join(','));
  const listedSource = $derived(np?.source ?? null);
  // Only on a new list: `np` and `listed` are new objects on every player update.
  $effect(() => {
    const source = listedSource;
    void listedKey;
    if (source) untrack(() => loadFavorites(source, listed));
  });

  // A row's menu: the whole menu (long-press on the row) or straight to its playlists (the row's heart). A track asked
  // for: its menu with Always accept from whoever asked (a user, not a guest).
  type RowMenu = { track: MenuTrack; start: 'menu' | 'lists'; queue: QueuePlace | null; asker?: string | null };
  let menuFor = $state<RowMenu | null>(null);
  // Shown or not; what it is for stays while it slides out.
  let menuShown = $state(false);
  const showMenu = (m: RowMenu) => ((menuFor = m), (menuShown = true));
  const asTrack = (t: { id: string; name: string; artist: string | null; album: string | null; album_id: string | null }, source: string): MenuTrack => ({
    source,
    id: t.id,
    name: t.name,
    artist: t.artist,
    album: t.album,
    album_id: t.album_id,
  });
  /** A row's menu, about its place in the queue; the playing track's card gives the playing track's own menu. */
  function rowMenu(track: MenuTrack, queue: QueuePlace | null, asker: string | null = null) {
    tick();
    showMenu({ track, start: 'menu', queue, asker });
  }

  async function play(i: number) {
    tick();
    try {
      await playUpcoming(i);
      list?.scrollTo({ top: 0 });
    } catch (e) {
      toast.fail(e, "The player didn't go to that track");
    }
  }

  // Dragged down from anywhere on it (the head, a row, the list at its top), it closes; a short drag springs back
  // (lib/sheetDrag). A row's swipe, the reorder grip and a held cover keep their own gestures.
  let sheet = $state<HTMLElement>();
  let pull = $state(0);

  // Reordering (this device; lib/reorder.svelte.ts): the queue changes on release.
  const reorder = new Reorder(
    () => list,
    () => np?.upcoming.length ?? 0,
    (from, to) => moveUpcoming(from, to),
  );
  const drag = $derived(reorder.drag);
  // A long press on a row lifts it to drag (the owner, 2026-10-26: its ⋮ opens the menu; the grip alone was hard to
  // find): from where it was pressed. The list does not scroll under the finger meanwhile.
  let pressed: { id: number; y: number; el: HTMLElement } | null = null;
  function liftRow(i: number) {
    if (!pressed) return;
    reorder.lift(pressed.el, pressed.id, pressed.y, i);
  }
  function noScrollWhileDragging(e: TouchEvent) {
    if (reorder.drag && e.cancelable) e.preventDefault();
  }
  $effect(() => {
    const l = list;
    if (!l) return;
    l.addEventListener('touchmove', noScrollWhileDragging, { passive: false });
    return () => l.removeEventListener('touchmove', noScrollWhileDragging);
  });

  // Swipe left removes (this device; lib/swipe.ts): the row slides out, then the track leaves the queue. Remove shows
  // in the strip it uncovers (lib/SwipeBehind).
  const REMOVE = { icon: 'trash', label: 'Remove', danger: true } as const;
  // A long press on the playing card's heart: Favourite and Exclude globally (lib/HeartSheet; no crate Excludes in the
  // Queue sheet, the owner).
  let heartMenu = $state(false);
  const removeSwiped = (i: number) => {
    setTimeout(() => removeUpcoming(i), reducedMotion() ? 0 : 200);
    return 'away' as const;
  };
  /** A long press on the play button: Start and stop's controls (lib/StartStopSheet; the owner, 2026-10-26). */
  let startStopOpen = $state(false);

  // Tracks asked for (0.10; the owner, 2026-10-30), on one's own player: each a row where it would go, its thumbs up
  // (into the queue, a row of it at once) and thumbs down (gone) in place of the heart and the handle, who asked on its
  // third line; shown or hidden from the head's thumb, its count on it (on while others may follow, unless chosen here).
  // One's own asked of the one followed show as rows too, waiting for them, until answered.
  const askedPlayer = $derived(playerShown());
  const askedCount = $derived(askedOf(askedPlayer));
  $effect(() => {
    const p = askedPlayer;
    void askedCount;
    if (p) untrack(() => loadAsked(p));
  });
  const showAsked = $derived(requestsShown(prefs.queueRequestsShown, mine.followable));
  const followed = $derived(followingNow());
  const asked = $derived(askedPlayer && showAsked ? (follow.asked[askedPlayer] ?? []) : []);
  const mineWaiting = $derived(followed ? waiting() : []);
  /** This device's queue to edit by hand: not while it follows someone (theirs is theirs). */
  const editable = $derived(here && !followed);
  const ownAsked = $derived(new Set(mineWaiting.map((r) => r.id)));
  // An accepted one stands in until the player's report has it, a while at most.
  $effect(() => {
    if (!follow.accepted.length) return;
    const t = setTimeout(() => (follow.accepted = follow.accepted.filter((a) => Date.now() - a.at < ACCEPTED_MS)), ACCEPTED_MS);
    return () => clearTimeout(t);
  });
  const accepted = $derived(np ? follow.accepted.filter((a) => a.r.player === askedPlayer && stillComing(a, np.upcoming, Date.now())).map((a) => a.r) : []);
  // While a row is dragged, only the queue's own rows (the reorder counts them).
  const lines = $derived(np ? (drag ? withRequests(np.upcoming, []) : withRequests(np.upcoming, [...asked, ...mineWaiting], accepted)) : []);
  /** Who asked stays on an accepted one's row (the owner, 2026-10-30: "Yes, that should stay on the row"): while it
   *  stands in here, then as the queue's own entry says (`asked_by`). */
  const ASKER_STAYS = true;
  const timesQueued = (id: string) => np?.upcoming.filter((t) => t.id === id).length ?? 0;
</script>

<!-- A row's cover; held, its track plays until the finger lifts (lib/preview.svelte.ts), ringed meanwhile. -->
{#snippet heldCover(t: { source: string; id: string; album_id: string | null; duration_secs?: number | null })}
  <span
    class="cover"
    class:hearing={preview.id === t.id}
    use:hold={{ start: () => startPreview(t), end: endPreview, enabled: canPreview() }}
  >
    <Cover src={t.album_id ? albumCoverUrl(t.source, t.album_id) : null} />
  </span>
{/snippet}

<!-- A row's second line: the artist, then the album (as Symfonium's), unless it is the album the queue plays. -->
{#snippet byline(t: { artist: string | null; album: string | null; source?: string })}
  <!-- Played from Bandcamp (the owner, 2026-10-26): said first. -->
  {#if (t.source ?? np?.source) === BANDCAMP}<b class="bc">Bandcamp</b>{[t.artist, t.album === from ? null : t.album].some(Boolean) ? ' • ' : ''}{/if}{[t.artist, t.album === from ? null : t.album].filter(Boolean).join(' • ')}
{/snippet}

<!-- A row's ⋮: its menu, as a long-press on the row. -->
{#snippet more(t: MenuTrack, place: QueuePlace)}
  <button class="more" aria-label="Menu of {t.name}" onclick={() => rowMenu(t, place)}><Icon name="moreVert" size={22} /></button>
{/snippet}

<!-- Finamp's row heart (lib/RowHeart.svelte): a tap opens its playlists, a long-press makes it a favourite or not. -->
{#snippet rowHeart(t: MenuTrack)}
  <!-- None for the device's own music (local mode, decision 0052). -->
  {#if t.source !== 'device'}<RowHeart source={t.source} id={t.id} name={t.name} artist={t.artist} cover={t.album_id ? albumCoverUrl(t.source, t.album_id) : null} onopen={() => showMenu({ track: t, start: 'lists', queue: null })} />{/if}
{/snippet}

{#if np}
  <div class="qback" class:screen role="presentation" onclick={onclose} transition:fade|global={{ duration: SHEET_OUT }}></div>
  <div
    class="queue"
    class:screen
    role="dialog"
    aria-label="Queue"
    bind:this={sheet}
    style:transform={pull ? `translateY(${pull}px)` : null}
    class:pulling={pull !== 0}
    use:sheetDrag={{ onclose, ondrag: (d) => (pull = d), enabled: !drag }}
    in:fly|global={{ y: 600, duration: reducedMotion() ? 0 : SHEET_IN, easing: decelerate }}
    out:fly|global={{ y: 600, duration: reducedMotion() ? 0 : SHEET_OUT, easing: decelerate }}
  >
    {#if src && !coverBroken(src)}<img class="blur" {src} alt="" aria-hidden="true" onerror={() => coverBroke(src)} />{/if}
    <div class="veil"></div>

    <div class="qhead">
      <span class="handle"></span>
      <div class="bar">
        {#if searching}
          <!-- The search in place, the row grown into a field, as the pages' search (lib/TopPill): Esc or leaving it
               empty folds it, ✕ empties it. -->
          <div class="find" role="search">
            <span class="lead" aria-hidden="true"><Icon name="search" size={20} /></span>
            <input
              type="search"
              placeholder="Title, artist or album"
              aria-label="Search the queue"
              bind:value={query}
              {@attach (el) => el.focus()}
              onkeydown={(e) => e.key === 'Escape' && ((searching = false), (query = ''))}
              onblur={() => query.trim() === '' && (searching = false)}
            />
            <button aria-label="Clear the search" title="Clear" onmousedown={(e) => e.preventDefault()} onclick={() => ((query = ''), (searching = false))}
              ><Icon name="close" size={20} /></button
            >
          </div>
        {:else}
          <span class="title">Queue</span>
          <span class="tools">
            <!-- The tracks asked for, shown or hidden, how many on it (0.10). -->
            {#if requestsOffered() && (mine.followable || askedCount > 0)}
              <button
                class="asked-toggle"
                class:lit={showAsked}
                aria-pressed={showAsked}
                aria-label={askedCount ? `Tracks asked for, ${askedCount}` : 'Tracks asked for'}
                title="Asked for"
                onclick={() => (prefs.queueRequestsShown = !showAsked)}
                ><Icon name="thumbsUp" size={22} />{#if askedCount}<span class="badge">{askedCount > 99 ? '99+' : askedCount}</span>{/if}</button
              >
            {/if}
            <!-- Search and the queue's menu (decision 0058: Shuffle, Save as playlist, Download…, Queues…, Clear). -->
            <button aria-label="Search the queue" onclick={() => (searching = true)}><Icon name="search" size={22} /></button>
            <button aria-label="Menu of the queue" title="Menu" onclick={() => (queueMenu = true)}><Icon name="moreVert" size={22} /></button>
          </span>
        {/if}
      </div>
    </div>

    <div class="list" bind:this={list}>
      {#if previous.length}
        <button class="sec prev" aria-expanded={prevOpen} onclick={() => (prevOpen = !prevOpen)}>
          <span>Previous Tracks</span><Icon name={prevOpen ? 'chevronUp' : 'collapse'} size={28} />
        </button>
        {#if prevOpen}
          <ol class="rows" transition:fade={{ duration: 150, easing: linear }}>
            {#each previous as t, j (t.id + ':' + j)}
              <li>
                <button class="row" onclick={() => (tick(), localJump(j))} use:longpress={() => rowMenu(asTrack(t, t.source), { list: 'previous', at: j })}>
                  {@render heldCover({ source: t.source, id: t.id, album_id: t.album_id, duration_secs: t.duration_secs })}
                  <div><strong>{t.name || '…'}</strong><span>{@render byline(t)}</span></div>
                </button>
                {@render rowHeart(asTrack(t, t.source))}
                {@render more(asTrack(t, t.source), { list: 'previous', at: j })}
              </li>
            {/each}
          </ol>
        {/if}
      {/if}

      <!-- The playing track: Finamp's card, filling in the cover's colour as it plays. -->
      <!-- The playing track: a long-press or right-click is its menu, as every row's (decision 0023). -->
      <div class="now" style:--done={dur ? Math.min(1, pos / dur) : 0} use:longpress={() => rowMenu(asTrack(np, np.source), null)}>
        <span class="done" aria-hidden="true"></span>
        <button class="ncover" aria-label={shownPaused() ? 'Play' : 'Pause'} aria-disabled={rampInert()} onclick={playPause} use:longpress={() => (startStopOpen = true)}>
          <!-- the stand-in only when there is no cover to load (lib/Cover), not while one loads -->
          {#if src || !coverSrc()}<Cover {src} lazy={false} />{/if}
          <span class="pp"><Icon name={shownPaused() ? 'play' : 'pause'} size={32} /></span>
        </button>
        <div class="ntext">
          <strong>{np.name}</strong>
          <span>{np.artist ?? ''}</span>
        </div>
        <span class="ntime">{clockText(pos)}{dur ? ` / ${clockText(dur)}` : ''}</span>
        {#if np.source === BANDCAMP}
          <span class="nbtn off" role="img" aria-label="On Bandcamp" title="On Bandcamp"><Icon name="heartOff" size={28} /></span>
        {:else if np.favorite !== null && np.source !== 'device'}
          <button
            class="nbtn"
            class:on={favorite()}
            disabled={pending()}
            aria-label={favorite() ? 'Favourite' : 'Not a favourite'}
            aria-pressed={favorite()}
            onclick={toggleFavorite}
            use:longpress={() => (heartMenu = true)}><Icon name="heart" size={28} /></button
          >
        {/if}
      </div>

      <div class="sec qsec">
        <div class="from">
          {#if from && fromView}<button class="chip" onclick={openFrom} use:longpress={() => (fromMenu = fromView)}><Icon name={fromView.startsWith('album:') ? 'albums' : fromView.startsWith(CRATE) ? 'crates' : 'playlists'} size={18} /><span>{from}</span></button>
          {:else if from && cratesComing}<!-- the crates on their way -->
          {:else if from}<span class="light">Playing from&nbsp;</span><strong>{from}</strong>
          {:else if !here && np.player}<span class="light">Playing on&nbsp;</span><strong>{np.player}</strong>
          {:else}<strong>Queue</strong>{/if}
        </div>
        <div class="count">{count}</div>
      </div>

      {#if lines.length}
        <ol class="rows" class:dragging={!!drag}>
          {#each lines as line (line.kind === 'track' ? line.t.id + ':' + line.i : `${line.kind}:${line.r.id}`)}
            {#if line.kind === 'track'}
            {@const t = line.t}
            {@const i = line.i}
            <!-- A line where the tracks added by hand end and the album or playlist goes on (decision 0033), and where
                 the radio's begin (decision 0036). -->
            <li
              hidden={!matches(t)}
              class:lifted={drag?.from === i}
              class:lane-end={i > 0 && np.upcoming[i - 1].lane !== t.lane && (np.upcoming[i - 1].lane === 'next' || t.lane === 'radio' || t.lane === 'autoplay')}
              style:transform={drag ? `translateY(${reorder.offset(i)}px)` : null}
            >
              {#if editable}<SwipeBehind left={REMOVE} />{/if}
              <button
                class="row"
                onclick={() => play(i)}
                onpointerdown={(e) => (pressed = { id: e.pointerId, y: e.clientY, el: e.currentTarget as HTMLElement })}
                onpointermove={reorder.move}
                onpointerup={reorder.end}
                onpointercancel={reorder.end}
                use:longpress={() => (editable && !q.length ? liftRow(i) : rowMenu(asTrack(t, np.source), { list: 'upcoming', at: i }))}
                use:swipe={{ enabled: editable && !drag, left: () => removeSwiped(i) }}
              >
                {@render heldCover({ source: np.source, id: t.id, album_id: t.album_id, duration_secs: t.duration_secs })}
                <div><strong>{t.name}</strong><span>{#if t.lane === 'radio'}<i class="radio" title="Radio"><Icon name="radio" size={14} /></i>{:else if t.lane === 'autoplay'}<i class="radio" title="Autoplay"><Icon name="infinity" size={14} /></i>{/if}{@render byline(t)}</span>{#if t.asked_by}<span class="who">Asked by {t.asked_by}</span>{/if}</div>
                {#if t.duration_secs}<span class="d">{clockText(t.duration_secs)}</span>{/if}
              </button>
              {@render rowHeart(asTrack(t, np.source))}
              {@render more(asTrack(t, np.source), { list: 'upcoming', at: i })}
              {#if editable && !q.length}
                <button
                  class="grip"
                  aria-label="Drag to reorder"
                  data-sheet-drag="off"
                  onpointerdown={(e) => reorder.down(e, i)}
                  onpointermove={reorder.move}
                  onpointerup={reorder.end}
                  onpointercancel={reorder.end}><Icon name="order" size={24} /></button
                >
              {/if}
            </li>
            {:else}
            <!-- A track asked for (0.10): a row as any, its thumbs up and down in place of the heart and the handle (an
                 accepted one has them back), who asked under its artist. One's own asked: waiting for the one followed. -->
            {@const r = line.r}
            {@const own = ownAsked.has(r.id)}
            {@const pending = line.kind === 'request'}
            <li hidden={!matches(r.track)} class:asked={pending}>
              <button
                class="row"
                onclick={tick}
                use:longpress={() => rowMenu(asTrack(r.track, r.track.source), null, !own && 'user' in r.from ? r.from.user : null)}
              >
                {@render heldCover({ source: r.track.source, id: r.track.id, album_id: r.track.album_id, duration_secs: r.track.duration_secs })}
                <div
                  ><strong>{r.track.name}</strong><span>{@render byline(r.track)}</span>{#if pending || ASKER_STAYS}<span class="who"
                      >{own ? `Waiting for ${followed?.user ?? ''}` : `Asked by ${askerName(r)}`}</span
                    >{/if}</div
                >
                {#if r.track.duration_secs}<span class="d">{clockText(r.track.duration_secs)}</span>{/if}
              </button>
              {#if pending && !own}
                <button class="thumb" aria-label="Accept {r.track.name}" title="Accept" onclick={() => (tick(), accept(r, timesQueued(r.track.id)))}><Icon name="thumbsUp" size={22} /></button>
              {:else if !pending}
                {@render rowHeart(asTrack(r.track, r.track.source))}
              {/if}
              <button class="more" aria-label="Menu of {r.track.name}" onclick={() => rowMenu(asTrack(r.track, r.track.source), null, !own && 'user' in r.from ? r.from.user : null)}
                ><Icon name="moreVert" size={22} /></button
              >
              {#if pending && !own}
                <button class="thumb" aria-label="Decline {r.track.name}" title="Decline" onclick={() => (heavy(), decline(r))}><Icon name="thumbsDown" size={22} /></button>
              {:else if !pending}
                <!-- Its handle once the queue has it (the player's next report). -->
                <span class="grip" aria-hidden="true"><Icon name="order" size={24} /></span>
              {/if}
            </li>
            {/if}
          {/each}
        </ol>
      {:else}
        <p class="empty">Nothing comes next.</p>
      {/if}
    </div>
    {#if here}
      <!-- More music (Radio and Autoplay), floating where the queue ends: what is on, a tap its sheet. -->
      <button class="more-music" class:lit={local.radio || local.autoplay !== null} aria-haspopup="dialog" aria-label="More music: {moreMusicLine()}" onclick={() => (moreOpen = true)}>
        {#each moreMusicParts() as p, i (p.icon)}{#if i}<span class="sep" aria-hidden="true">·</span>{/if}<Icon name={p.icon} size={20} /><span>{p.text}</span>{:else}<Icon
            name="radio"
            size={20}
          /><span>Off</span>{/each}
      </button>
    {/if}
  </div>
{/if}



{#if moreOpen}
  <MoreMusicSheet onclose={() => (moreOpen = false)} />
{/if}

{#if menuShown && menuFor}
  <TrackMenu
    track={menuFor.track}
    start={menuFor.start}
    queue={menuFor.queue}
    asker={menuFor.asker ?? null}
    onclose={() => (menuShown = false)}
  />
{/if}
{#if saving}
  <QueuePlaylistSheet onclose={() => (saving = false)} />
{/if}
{#if queueMenu}
  <QueueMenu onclose={() => (queueMenu = false)} />
{/if}
{#if fromMenu}
  <SubjectMenu subject={fromMenu} onclose={() => (fromMenu = null)} />
{/if}
{#if heartMenu && np}
  <HeartSheet track={np} cover={src} favourite={{ on: favorite(), disabled: pending() || np.favorite === null, flip: toggleFavorite }} onclose={() => (heartMenu = false)} />
{/if}
{#if switching}
  <QueuesSheet onclose={() => (switching = false)} />
{/if}

{#if startStopOpen}<StartStopSheet onclose={() => (startStopOpen = false)} />{/if}

<style>
  .qback {
    position: absolute;
    inset: 0;
    z-index: 2;
    background: rgb(0 0 0 / 0.54);
  }

  /* Finamp's sheet: 92 % of the height, 90 % of the width between 640 and 900 px (all of it on a phone), corners 24. */
  .queue {
    position: absolute;
    inset: 8% 0 0;
    z-index: 2;
    width: min(100%, clamp(640px, 90%, 900px));
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-radius: 24px 24px 0 0;
    background: var(--surface);
    box-shadow: 0 -8px 30px rgb(0 0 0 / 0.4);
    transition: transform 200ms linear;
  }

  /* Over the whole window (from the mini-player), on the layer the other sheets use (lib/MenuSheet). */
  .qback.screen {
    position: fixed;
    z-index: 40;
  }

  .queue.screen {
    position: fixed;
    z-index: 41;
  }

  .queue.pulling {
    transition: none;
  }

  .blur {
    position: absolute;
    inset: -20%;
    width: 140%;
    height: 140%;
    object-fit: cover;
    filter: blur(60px) saturate(1.2);
    pointer-events: none;
  }

  .veil {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--surface) 67.5%, transparent);
    pointer-events: none;
  }

  .qhead {
    position: relative;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 10px 0 20px;
    touch-action: none;
    cursor: grab;
  }

  .handle {
    width: 40px;
    height: 3.5px;
    margin-bottom: 10px;
    border-radius: 3.5px;
    background: var(--muted);
  }

  /* The head's row: "Queue", then its tools (search, Clear); searching, the field in their place. */
  .bar {
    display: flex;
    align-items: center;
    align-self: stretch;
    gap: 8px;
    padding: 0 8px 0 18px;
  }

  .title {
    flex: 1;
    font-size: 18px;
    font-weight: 400;
  }

  .tools {
    display: flex;
  }

  .tools button {
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


  .find {
    flex: 1;
    display: flex;
    align-items: center;
    min-width: 0;
    height: 44px;
    margin-left: -10px;
    border-radius: 22px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .find .lead {
    display: grid;
    place-items: center;
    width: 40px;
    color: var(--muted);
  }

  .find input {
    flex: 1;
    min-width: 0;
    height: 44px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 16px;
    outline: none;
  }

  .find input::-webkit-search-cancel-button {
    display: none;
  }

  .find button {
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

  /* More music: a pill floating over the list's lower right corner, lit in the cover's accent while Radio or Autoplay
     is on. */
  .more-music {
    position: absolute;
    right: 16px;
    bottom: calc(20px + env(safe-area-inset-bottom));
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: calc(100% - 32px);
    height: 48px;
    padding: 0 18px 0 14px;
    border: 0;
    border-radius: 24px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.35);
    color: var(--muted);
    font: inherit;
    font-size: 15px;
    cursor: pointer;
  }

  .more-music span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .more-music :global(svg) {
    flex: none;
  }

  .more-music .sep {
    margin: 0 2px;
  }

  .more-music.lit {
    background: var(--art-accent);
    color: var(--accent-text);
  }

  /* "Playing from" as a chip that opens it. */
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    max-width: 100%;
    min-height: 36px;
    padding: 0 14px 0 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .chip span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .more {
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
  }

  .tools button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .list {
    position: relative;
    flex: 1;
    overflow-y: auto;
    padding-bottom: calc(90px + env(safe-area-inset-bottom));
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--art-accent) 70%, transparent) color-mix(in srgb, var(--art-accent) 20%, transparent);
  }

  .sec {
    display: flex;
    align-items: center;
    width: 100%;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 14px;
    text-align: left;
  }

  /* "Previous Tracks": 50 px, a chevron after the text. */
  .prev {
    gap: 4px;
    height: 50px;
    padding: 12px 14px 12px;
    cursor: pointer;
  }

  /* The playing track: pinned at the top, 70 px, corners 12; the part played in the cover's colour, the rest the
     colour at 70 % over the background. */
  .now {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 70px;
    margin: 0 8px;
    overflow: hidden;
    border-radius: 12px;
    background: color-mix(in srgb, var(--art-accent) 70%, var(--bg));
  }

  .done {
    position: absolute;
    inset: 0 auto 0 0;
    width: calc(var(--done) * 100%);
    border-radius: 0 12px 12px 0;
    background: var(--art-accent);
    pointer-events: none;
  }

  .ncover {
    position: relative;
    flex: none;
    width: 70px;
    height: 70px;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }

  .pp {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: white;
    filter: drop-shadow(0 0 5px rgb(0 0 0 / 0.8));
  }

  .ntext {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .ntext strong,
  .ntext span {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ntext strong {
    font-size: 16px;
    line-height: 1.3;
    font-weight: 600;
  }

  .ntext span {
    font-size: 13px;
  }

  .ntime {
    position: relative;
    flex: none;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }

  .nbtn {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .nbtn:last-child {
    margin-right: 4px;
  }

  .nbtn.on :global(svg) {
    fill: currentColor;
  }

  .nbtn:disabled {
    opacity: 0.5;
  }

  /* "Playing from …" and "i / n (remaining)", pinned under the playing track. */
  .qsec {
    position: sticky;
    top: 70px;
    z-index: 1;
    flex-direction: column;
    align-items: flex-start;
    padding: 12.5px 12px 8px;
    /* Finamp's headers have no background: the rows fade out under them. Here they blur out under a light veil. */
    background: color-mix(in srgb, var(--surface) 35%, transparent);
    backdrop-filter: blur(16px);
    mask-image: linear-gradient(black calc(100% - 10px), transparent);
  }

  .from {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .from .light {
    font-weight: 300;
  }

  .from strong {
    font-weight: 500;
  }

  .count {
    padding: 4px 8px 0 0;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    white-space: pre;
  }

  /* Finamp's queue rows: 70 px, a 60 px cover with corners 8, the title up to two lines, the artist under it. */
  .rows {
    position: relative;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows li {
    position: relative;
    display: flex;
    align-items: center;
    height: 70px;
    padding: 6px 8px 0;
    box-sizing: border-box;
  }

  /* The radio's and Autoplay's tracks carry their mark before the artist (decision 0036, amended). */
  .row .radio {
    display: inline-block;
    margin-right: 4px;
    vertical-align: -2px;
  }

  .rows li[hidden] {
    display: none;
  }

  /* Takes no room, so dragging counts rows as before. */
  .rows li.lane-end::before {
    content: '';
    position: absolute;
    top: 1px;
    left: 16px;
    right: 16px;
    border-top: 1px solid var(--border);
  }

  .rows.dragging li {
    transition: transform 200ms ease;
  }

  .rows.dragging li.lifted {
    position: relative;
    z-index: 1;
    transition: none;
  }

  .row {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 100%;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row:hover,
  .row:focus-visible {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  /* A cover is its own press: held, it plays its track (ringed meanwhile); no image menu, no drag. */
  .cover {
    position: relative;
    flex: none;
    display: block;
    width: 60px;
    height: 60px;
    border-radius: 8px;
    -webkit-touch-callout: none;
    user-select: none;
    touch-action: pan-y;
  }

  .cover.hearing {
    box-shadow: 0 0 0 3px var(--art-accent);
  }

  .row div {
    flex: 1;
    min-width: 0;
  }

  .row strong {
    display: -webkit-box;
    overflow: hidden;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    font-size: 15.5px;
    font-weight: 500;
    line-height: 1.1;
  }

  .row div span {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    opacity: 0.75;
  }

  .d {
    flex: none;
    padding-right: 4px;
    color: var(--muted);
    opacity: 0.85;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  /* A track asked for: its thumbs where the heart and the handle are (0.10). */
  .thumb {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
  }

  .row div span.who {
    font-size: 12px;
    opacity: 0.6;
  }

  .tools .asked-toggle {
    position: relative;
  }

  .tools .asked-toggle:not(.lit) {
    opacity: 0.55;
  }

  .tools .badge {
    position: absolute;
    top: 2px;
    right: 0;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font-size: 11px;
    font-weight: 700;
    line-height: 16px;
    text-align: center;
  }

  .grip {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 100%;
    margin-left: 4px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    cursor: grab;
    touch-action: none;
  }

  .rows.dragging .grip {
    cursor: grabbing;
  }

  .empty {
    margin: 16px;
    color: var(--muted);
  }

  /* Played from Bandcamp: said first in its line. */
  .bc {
    color: var(--accent-ink);
    font-weight: 600;
  }

  .nbtn.off {
    color: var(--muted);
    cursor: default;
  }
</style>
