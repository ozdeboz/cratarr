<script lang="ts">
  // The queue (decisions 0019, 0022), as Finamp's queue sheet (lib/components/PlayerScreen/queue_list.dart): a sheet
  // over the player on the blurred cover, a grab handle and "Queue"; then the previous tracks (folded, this device
  // only: the server's player reports only what comes next), the playing track as a card that fills as it plays
  // (pinned at the top), the "Playing from" header (pinned under it) and what comes next. From Symfonium's queue
  // (decided 2026-10-16): search, shuffle, repeat, the radio and Clear (what comes next goes, with Undo) in the head;
  // when the queue ends on the clock; "Playing from" a chip that opens it; a row's album after its artist (not the
  // album the queue plays), and its ⋮.
  // Gestures as Finamp's: drag the handle area down to close; a tap on a row plays it, a long-press (or right-click) is
  // its menu; a row's heart adds it to a playlist, a long-press on the heart makes it a favourite or not; on this device
  // a row's handle reorders and a swipe left removes it (the server's player takes neither command). Haptics as
  // Finamp's: light on a tap or a drag starting, heavy on a drop, a removal and opening.
  import { untrack } from 'svelte';
  import { linear } from 'svelte/easing';
  import { fade, fly } from 'svelte/transition';
  import { albumCoverUrl } from './api';
  import { loadFavorites } from './favourites.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import RowHeart from './RowHeart.svelte';
  import { swipe } from './swipe';
  import { Reorder } from './reorder.svelte';
  import TrackMenu, { type MenuTrack, type QueuePlace } from './TrackMenu.svelte';
  import { clearUpcoming, local, jump as localJump, moveUpcoming, removeUpcoming, restoreUpcoming, setRepeat, setShuffle } from './localPlayer.svelte';
  import RadioSheet from './RadioSheet.svelte';
  import { albumSubject, viewPlaylist } from './panel.svelte';
  import { crates, CRATE } from './crates.svelte';
  import { player } from './live.svelte';
  import { decelerate, reducedMotion, SHEET_IN, SHEET_OUT } from './motion';
  import { isLocal, shownPlaylists } from './output.svelte';
  import { clockText, duration, favorite, paused, pending, playUpcoming, playing, position, send, toggleFavorite } from './remote.svelte';
  import { toast } from './toast.svelte';

  let { src, onclose }: { src: string | null; onclose: () => void } = $props();

  const np = $derived(playing());
  const pos = $derived(position());
  const dur = $derived(duration());
  const here = $derived(isLocal());
  // Finamp's haptics: a selection tick and a heavy one.
  const tick = () => navigator.vibrate?.(8);
  const heavy = () => navigator.vibrate?.(25);
  heavy();

  // The previous tracks: folded, as Finamp's default.
  let prevOpen = $state(false);
  const previous = $derived(here ? local.queue.slice(0, Math.max(0, local.index)) : []);

  // The queue section's header: where the queue came from, and "i / n (remaining)".
  const from = $derived(here ? local.from : null);
  const count = $derived.by(() => {
    if (!np) return '';
    const n = here ? local.queue.length : np.upcoming.length + 1;
    const i = here ? local.index + 1 : 1;
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
    const c = crates.list?.find((x) => x.name === from);
    return c ? CRATE + c.id : null;
  });
  function openFrom() {
    const v = fromView;
    if (!v) return;
    onclose();
    player.full = false;
    viewPlaylist(v);
  }

  // Search: what comes next, by title, artist or album; the rows that match show (no reordering meanwhile).
  let searching = $state(false);
  let query = $state('');
  const q = $derived(searching ? query.trim().toLowerCase() : '');
  const matches = (t: { name: string; artist: string | null; album: string | null }) =>
    !q || [t.name, t.artist, t.album].some((x) => x?.toLowerCase().includes(q));

  // The radio's sheet, from the head (as the player's radio button).
  let radioOpen = $state(false);

  // Clear: what comes next goes; the toast's Undo puts it back.
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

  // A row's menu: the whole menu (long-press on the row) or straight to its playlists (the row's heart).
  let menuFor = $state<{ track: MenuTrack; start: 'menu' | 'lists'; queue: QueuePlace | null } | null>(null);
  // Shown or not; what it is for stays while it slides out.
  let menuShown = $state(false);
  const showMenu = (m: { track: MenuTrack; start: 'menu' | 'lists'; queue: QueuePlace | null }) => ((menuFor = m), (menuShown = true));
  const asTrack = (t: { id: string; name: string; artist: string | null; album: string | null; album_id: string | null }, source: string): MenuTrack => ({
    source,
    id: t.id,
    name: t.name,
    artist: t.artist,
    album: t.album,
    album_id: t.album_id,
  });
  /** A row's menu, about its place in the queue; the playing track's card gives the playing track's own menu. */
  function rowMenu(track: MenuTrack, queue: QueuePlace | null) {
    tick();
    showMenu({ track, start: 'menu', queue });
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

  // Closing by dragging the handle area down: past half the sheet or with a flick (Flutter's sheet thresholds), else it
  // springs back.
  let sheet = $state<HTMLElement>();
  let pull = $state(0);
  let pullFrom = $state<{ y: number; t: number } | null>(null);
  function pullDown(e: PointerEvent) {
    if ((e.target as Element).closest('button, input')) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    pullFrom = { y: e.clientY, t: performance.now() };
  }
  function pullMove(e: PointerEvent) {
    if (pullFrom) pull = Math.max(0, e.clientY - pullFrom.y);
  }
  function pullUp(e: PointerEvent) {
    if (!pullFrom) return;
    const v = pull / Math.max(1, performance.now() - pullFrom.t);
    pullFrom = null;
    if (pull > (sheet?.offsetHeight ?? 600) / 2 || (pull > 20 && v > 0.7)) onclose();
    else pull = 0;
  }

  // Reordering (this device; lib/reorder.svelte.ts): the queue changes on release.
  const reorder = new Reorder(
    () => list,
    () => np?.upcoming.length ?? 0,
    (from, to) => moveUpcoming(from, to),
  );
  const drag = $derived(reorder.drag);

  // Swipe left removes (this device; lib/swipe.ts): the row slides out, then the track leaves the queue.
  const removeSwiped = (i: number) => {
    setTimeout(() => removeUpcoming(i), reducedMotion() ? 0 : 200);
    return 'away' as const;
  };
</script>

<!-- A row's second line: the artist, then the album (as Symfonium's), unless it is the album the queue plays. -->
{#snippet byline(t: { artist: string | null; album: string | null })}
  {[t.artist, t.album === from ? null : t.album].filter(Boolean).join(' • ')}
{/snippet}

<!-- A row's ⋮: its menu, as a long-press on the row. -->
{#snippet more(t: MenuTrack, place: QueuePlace)}
  <button class="more" aria-label="Menu of {t.name}" onclick={() => rowMenu(t, place)}><Icon name="moreVert" size={22} /></button>
{/snippet}

<!-- Finamp's row heart (lib/RowHeart.svelte): a tap opens its playlists, a long-press makes it a favourite or not. -->
{#snippet rowHeart(t: MenuTrack)}
  <RowHeart source={t.source} id={t.id} name={t.name} onopen={() => showMenu({ track: t, start: 'lists', queue: null })} />
{/snippet}

{#if np}
  <div class="qback" role="presentation" onclick={onclose} transition:fade={{ duration: SHEET_OUT }}></div>
  <div
    class="queue"
    role="dialog"
    aria-label="Queue"
    bind:this={sheet}
    style:transform={pull ? `translateY(${pull}px)` : null}
    class:pulling={pullFrom !== null}
    in:fly={{ y: 600, duration: reducedMotion() ? 0 : SHEET_IN, easing: decelerate }}
    out:fly={{ y: 600, duration: reducedMotion() ? 0 : SHEET_OUT, easing: decelerate }}
  >
    {#if src}<img class="blur" {src} alt="" aria-hidden="true" />{/if}
    <div class="veil"></div>

    <div class="qhead" role="presentation" onpointerdown={pullDown} onpointermove={pullMove} onpointerup={pullUp} onpointercancel={pullUp}>
      <span class="handle"></span>
      <div class="bar">
        <span class="title">Queue</span>
        <span class="tools">
          <button class:on={searching} aria-label="Search the queue" aria-pressed={searching} onclick={() => ((searching = !searching), (query = ''))}
            ><Icon name="search" size={22} /></button
          >
          {#if here}
            <button class:on={local.shuffled} aria-label="Shuffle" aria-pressed={local.shuffled} onclick={() => setShuffle(!local.shuffled)}><Icon name="shuffle" size={22} /></button>
            <button
              class:on={local.repeat !== 'off'}
              aria-label={local.repeat === 'one' ? 'Repeat one' : local.repeat === 'all' ? 'Repeat all' : 'Repeat'}
              aria-pressed={local.repeat !== 'off'}
              onclick={() => setRepeat(local.repeat === 'off' ? 'all' : local.repeat === 'all' ? 'one' : 'off')}
              ><Icon name={local.repeat === 'one' ? 'repeatOne' : 'repeat'} size={22} /></button
            >
            <button class:on={local.radio !== null} aria-label="Radio" aria-haspopup="dialog" aria-pressed={local.radio !== null} onclick={() => (radioOpen = true)}
              ><Icon name="radio" size={22} /></button
            >
            <button aria-label="Clear the queue" disabled={!np.upcoming.length} onclick={clear}><Icon name="clearQueue" size={22} /></button>
          {/if}
        </span>
      </div>
      {#if searching}
        <input class="find" type="search" placeholder="Title, artist or album" aria-label="Search the queue" bind:value={query} {@attach (el) => el.focus()} />
      {/if}
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
                  {#if t.album_id}<img src={albumCoverUrl(t.source, t.album_id)} alt="" loading="lazy" />{:else}<span class="noimg"></span>{/if}
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
        <button class="ncover" aria-label={paused() ? 'Play' : 'Pause'} onclick={() => send('play-pause')}>
          {#if src}<img {src} alt="" />{:else}<span class="noimg"></span>{/if}
          <span class="pp"><Icon name={paused() ? 'play' : 'pause'} size={32} /></span>
        </button>
        <div class="ntext">
          <strong>{np.name}</strong>
          <span>{np.artist ?? ''}</span>
        </div>
        <span class="ntime">{clockText(pos)}{dur ? ` / ${clockText(dur)}` : ''}</span>
        {#if np.favorite !== null}
          <button class="nbtn" class:on={favorite()} disabled={pending()} aria-label={favorite() ? 'Favourite' : 'Not a favourite'} aria-pressed={favorite()} onclick={toggleFavorite}
            ><Icon name="heart" size={28} /></button
          >
        {/if}
      </div>

      <div class="sec qsec">
        <div class="from">
          {#if from && fromView}<button class="chip" onclick={openFrom}><Icon name={fromView.startsWith('album:') ? 'albums' : fromView.startsWith(CRATE) ? 'crates' : 'playlists'} size={18} /><span>{from}</span></button>
          {:else if from}<span class="light">Playing from&nbsp;</span><strong>{from}</strong>
          {:else if !here && np.player}<span class="light">Playing on&nbsp;</span><strong>{np.player}</strong>
          {:else}<strong>Queue</strong>{/if}
        </div>
        <div class="count">{count}</div>
      </div>

      {#if np.upcoming.length}
        <ol class="rows" class:dragging={!!drag}>
          {#each np.upcoming as t, i (t.id + ':' + i)}
            <!-- A line where the tracks added by hand end and the album or playlist goes on (decision 0033), and where
                 the radio's begin (decision 0036). -->
            <li
              hidden={!matches(t)}
              class:lifted={drag?.from === i}
              class:lane-end={i > 0 && np.upcoming[i - 1].lane !== t.lane && (np.upcoming[i - 1].lane === 'next' || t.lane === 'radio')}
              style:transform={drag ? `translateY(${reorder.offset(i)}px)` : null}
            >
              <button
                class="row"
                onclick={() => play(i)}
                use:longpress={() => rowMenu(asTrack(t, np.source), { list: 'upcoming', at: i })}
                use:swipe={{ enabled: here && !drag, onswipe: () => removeSwiped(i) }}
              >
                {#if t.album_id}<img src={albumCoverUrl(np.source, t.album_id)} alt="" loading="lazy" />{:else}<span class="noimg"></span>{/if}
                <div><strong>{t.name}</strong><span>{#if t.lane === 'radio'}<i class="radio" title="Radio"><Icon name="radio" size={14} /></i>{/if}{@render byline(t)}</span></div>
                {#if t.duration_secs}<span class="d">{clockText(t.duration_secs)}</span>{/if}
              </button>
              {@render rowHeart(asTrack(t, np.source))}
              {@render more(asTrack(t, np.source), { list: 'upcoming', at: i })}
              {#if here && !q}
                <button
                  class="grip"
                  aria-label="Drag to reorder"
                  onpointerdown={(e) => reorder.down(e, i)}
                  onpointermove={reorder.move}
                  onpointerup={reorder.end}
                  onpointercancel={reorder.end}><Icon name="order" size={24} /></button
                >
              {/if}
            </li>
          {/each}
        </ol>
      {:else}
        <p class="empty">Nothing comes next.</p>
      {/if}
    </div>
  </div>
{/if}

{#if radioOpen}
  <RadioSheet onclose={() => (radioOpen = false)} />
{/if}

{#if menuShown && menuFor}
  <TrackMenu
    track={menuFor.track}
    start={menuFor.start}
    queue={menuFor.queue}
    onclose={() => (menuShown = false)}
  />
{/if}

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

  /* The head's row: "Queue", then its tools (Symfonium's: search, shuffle, repeat, the radio), lit while on. */
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

  .tools button.on {
    color: var(--art-accent);
  }

  .find {
    align-self: stretch;
    min-height: 40px;
    margin: 8px 12px 0;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg) 60%, transparent);
    color: inherit;
    font: inherit;
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

  .ncover img,
  .ncover .noimg {
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: 0;
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

  /* The radio's tracks carry its mark before the artist (decision 0036). */
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

  .row img,
  .noimg {
    flex: none;
    width: 60px;
    height: 60px;
    border-radius: 8px;
    object-fit: cover;
    background: var(--bg);
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
</style>
