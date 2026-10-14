<script lang="ts">
  // The queue (decisions 0019, 0022), as Finamp's queue sheet (lib/components/PlayerScreen/queue_list.dart): a sheet
  // over the player on the blurred cover, a grab handle and "Queue"; then the previous tracks (folded, this device
  // only: the server's player reports only what comes next), the playing track as a card that fills as it plays
  // (pinned at the top), the "Playing from" header (pinned under it) and what comes next.
  // Gestures as Finamp's: drag the handle area down to close; a tap on a row plays it; on this device a row's handle
  // reorders and a swipe left removes it (the server's player takes neither command). Haptics as Finamp's: light on a
  // tap or a drag starting, heavy on a drop, a removal and opening.
  import { linear } from 'svelte/easing';
  import { fade, fly } from 'svelte/transition';
  import { incomingCoverUrl } from './api';
  import Icon from './Icon.svelte';
  import { local, jump as localJump, moveUpcoming, removeUpcoming } from './localPlayer.svelte';
  import { decelerate, reducedMotion, SHEET_IN, SHEET_OUT } from './motion';
  import { isLocal } from './output.svelte';
  import { clockText, duration, favorite, paused, pending, playUpcoming, playing, position, send, toggleFavorite } from './remote.svelte';
  import { toast } from './toast.svelte';

  let { src, onclose, onmenu }: { src: string | null; onclose: () => void; onmenu: (e: Event) => void } = $props();

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
    return `${i} / ${n}${left !== null ? `  (${clockText(left)})` : ''}`;
  });

  let list = $state<HTMLElement>();

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

  // Reordering (this device): Finamp's handle starts the drag at once; the row follows the finger, the rows it passes
  // make room, the list scrolls near its edges, the queue changes on release.
  let drag = $state<{ from: number; to: number; dy: number } | null>(null);
  let dragStart = { y: 0, scroll: 0, rowH: 70 };
  let scrollTimer: ReturnType<typeof setInterval> | undefined;
  let lastY = 0;
  function dragDown(e: PointerEvent, i: number) {
    if (!here || !list) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    dragStart = { y: e.clientY, scroll: list.scrollTop, rowH: el.closest('li')!.getBoundingClientRect().height };
    drag = { from: i, to: i, dy: 0 };
    tick();
  }
  function dragTo(y: number) {
    if (!drag || !list || !np) return;
    const dy = y - dragStart.y + (list.scrollTop - dragStart.scroll);
    const to = Math.min(np.upcoming.length - 1, Math.max(0, drag.from + Math.round(dy / dragStart.rowH)));
    drag = { ...drag, dy, to };
  }
  function dragMove(e: PointerEvent) {
    if (!drag || !list) return;
    lastY = e.clientY;
    dragTo(lastY);
    const r = list.getBoundingClientRect();
    const speed = lastY < r.top + 48 ? -8 : lastY > r.bottom - 48 ? 8 : 0;
    clearInterval(scrollTimer);
    if (speed)
      scrollTimer = setInterval(() => {
        list!.scrollTop += speed;
        dragTo(lastY);
      }, 16);
  }
  function dragEnd() {
    clearInterval(scrollTimer);
    if (!drag) return;
    const { from, to } = drag;
    drag = null;
    heavy();
    if (from !== to) moveUpcoming(from, to);
  }
  function offset(j: number): number {
    if (!drag) return 0;
    const { from, to, dy } = drag;
    if (j === from) return dy;
    if (from < to && j > from && j <= to) return -dragStart.rowH;
    if (to < from && j >= to && j < from) return dragStart.rowH;
    return 0;
  }

  // Swipe left removes (this device): the row follows the finger; past 65 % of its width it goes, else it springs
  // back. Vertical moves stay the list's scrolling; a swipe never also counts as a tap.
  let swipe = $state<{ i: number; dx: number; out: boolean } | null>(null);
  let swipeFrom = $state<{ i: number; x: number; y: number; w: number; on: boolean } | null>(null);
  let swiped = false;
  function rowDown(e: PointerEvent, i: number) {
    if (!here || drag) return;
    swipeFrom = { i, x: e.clientX, y: e.clientY, w: (e.currentTarget as HTMLElement).offsetWidth, on: false };
  }
  function rowMove(e: PointerEvent) {
    const s = swipeFrom;
    if (!s) return;
    const [dx, dy] = [e.clientX - s.x, e.clientY - s.y];
    if (!s.on) {
      if (Math.abs(dy) > 10 && Math.abs(dy) > Math.abs(dx)) return void (swipeFrom = null);
      if (dx > -10 || Math.abs(dx) < Math.abs(dy)) return;
      s.on = true;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    }
    swipe = { i: s.i, dx: Math.min(0, dx), out: false };
  }
  function rowUp() {
    const s = swipeFrom;
    swipeFrom = null;
    if (!s?.on || !swipe) return;
    swiped = true;
    if (-swipe.dx > s.w * 0.65) {
      heavy();
      swipe = { i: s.i, dx: -s.w, out: true };
      setTimeout(() => {
        removeUpcoming(s.i);
        swipe = null;
      }, reducedMotion() ? 0 : 200);
    } else {
      swipe = { i: s.i, dx: 0, out: false };
      setTimeout(() => (swipe = null), 200);
    }
  }
  function rowClick(i: number) {
    if (swiped) return void (swiped = false);
    play(i);
  }
  const rowX = (i: number) => (swipe?.i === i ? swipe.dx : 0);
</script>

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
      <span class="title">Queue</span>
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
                <button class="row" onclick={() => (tick(), localJump(j))}>
                  {#if t.album_id}<img src={incomingCoverUrl(t.source, t.album_id)} alt="" loading="lazy" />{:else}<span class="noimg"></span>{/if}
                  <div><strong>{t.name || '…'}</strong><span>{t.artist ?? ''}</span></div>
                </button>
              </li>
            {/each}
          </ol>
        {/if}
      {/if}

      <!-- The playing track: Finamp's card, filling in the cover's colour as it plays. -->
      <div class="now" style:--done={dur ? Math.min(1, pos / dur) : 0}>
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
        <button class="nbtn" aria-label="Track menu" aria-haspopup="menu" onclick={onmenu}><Icon name="more" size={28} /></button>
      </div>

      <div class="sec qsec">
        <div class="from">
          {#if from}<span class="light">Playing from&nbsp;</span><strong>{from}</strong>
          {:else if !here && np.player}<span class="light">Playing on&nbsp;</span><strong>{np.player}</strong>
          {:else}<strong>Queue</strong>{/if}
        </div>
        <div class="count">{count}</div>
      </div>

      {#if np.upcoming.length}
        <ol class="rows" class:dragging={!!drag}>
          {#each np.upcoming as t, i (t.id + ':' + i)}
            <li
              class:lifted={drag?.from === i}
              class:gone={swipe?.i === i && swipe.out}
              style:transform={drag ? `translateY(${offset(i)}px)` : null}
            >
              <button
                class="row"
                class:moving={swipe?.i === i && swipeFrom !== null}
                style:transform={rowX(i) ? `translateX(${rowX(i)}px)` : null}
                onpointerdown={(e) => rowDown(e, i)}
                onpointermove={rowMove}
                onpointerup={rowUp}
                onpointercancel={rowUp}
                onclick={() => rowClick(i)}
              >
                {#if t.album_id}<img src={incomingCoverUrl(np.source, t.album_id)} alt="" loading="lazy" />{:else}<span class="noimg"></span>{/if}
                <div><strong>{t.name}</strong><span>{t.artist ?? ''}</span></div>
                {#if t.duration_secs}<span class="d">{clockText(t.duration_secs)}</span>{/if}
              </button>
              {#if here}
                <button
                  class="grip"
                  aria-label="Drag to reorder"
                  onpointerdown={(e) => dragDown(e, i)}
                  onpointermove={dragMove}
                  onpointerup={dragEnd}
                  onpointercancel={dragEnd}><Icon name="order" size={24} /></button
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

  .title {
    font-size: 18px;
    font-weight: 400;
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
    display: flex;
    align-items: center;
    height: 70px;
    padding: 6px 8px 0;
    box-sizing: border-box;
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
    touch-action: pan-y;
    transition: transform 200ms linear;
  }

  .row.moving {
    transition: none;
  }

  .row:hover,
  .row:focus-visible {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  li.gone {
    opacity: 0;
    transition: opacity 200ms linear;
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
