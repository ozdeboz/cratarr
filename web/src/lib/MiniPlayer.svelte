<script lang="ts">
  // The mini-player along the bottom (decision 0018), as Finamp's now-playing bar: a floating bar with the cover and
  // play/pause on it, the title (scrolling when it does not fit), artist and time, and the heart. The whole bar fills
  // from the left as the track plays, in the cover's colour. Tap or swipe up: the full player (beside the page when
  // there is room, else over it). Swipe left or right: next or previous; the bar follows the finger and springs
  // back. Swipe down does nothing (Finamp stops and clears the queue; not on a remote across the room).
  import Icon from './Icon.svelte';
  import { live, player } from './live.svelte';
  import { prefs } from './prefs.svelte';
  import { art, clockText, coverFailed, coverSrc, duration, favorite, paused, position, send, toggleFavorite } from './remote.svelte';

  const np = $derived(live.view?.now_playing ?? null);
  const src = $derived(coverSrc());
  const pos = $derived(position());
  const dur = $derived(duration());
  const share = $derived(dur ? Math.min(1, pos / dur) : 0);

  /** The full player: beside the page when the window has room and that is the chosen place, else over it. */
  function openPlayer() {
    if (player.roomy && prefs.sidebar) player.min = false;
    else player.full = true;
  }

  // Swipes: horizontal ones move the bar with the finger and skip past 25 % of its width (Finamp's Dismissible);
  // upward ones open the player past 15 % of its height; a tap opens it too.
  let dx = $state(0);
  let dragging = $state(false);
  let from: { x: number; y: number; id: number } | null = null;
  let moved = false;
  let el = $state<HTMLElement>();

  function down(e: PointerEvent) {
    if ((e.target as Element).closest('.own')) return; // the play button and the heart act on their own
    from = { x: e.clientX, y: e.clientY, id: e.pointerId };
    moved = false;
  }
  function move(e: PointerEvent) {
    if (!from || e.pointerId !== from.id) return;
    const [mx, my] = [e.clientX - from.x, e.clientY - from.y];
    if (!dragging && Math.abs(mx) > 8 && Math.abs(mx) > Math.abs(my)) {
      dragging = true;
      el?.setPointerCapture(e.pointerId);
    }
    if (dragging) dx = mx;
    if (Math.abs(mx) > 8 || Math.abs(my) > 8) moved = true;
  }
  function up(e: PointerEvent) {
    if (!from || e.pointerId !== from.id) return;
    const [mx, my] = [e.clientX - from.x, e.clientY - from.y];
    const width = el?.clientWidth ?? 300;
    from = null;
    if (dragging && Math.abs(mx) > width * 0.25) send(mx < 0 ? 'next' : 'previous');
    else if (!dragging && my < -64 * 0.15 * 2 && -my > Math.abs(mx)) openPlayer();
    else if (!moved) openPlayer();
    dragging = false;
    dx = 0;
  }

  // The title scrolls when it does not fit (Finamp's marquee).
  let titleBox = $state<HTMLElement>();
  let titleText = $state<HTMLElement>();
  let overflow = $state(0);
  $effect(() => {
    void np?.name;
    if (!titleBox || !titleText) return;
    const measure = () => (overflow = Math.max(0, titleText!.scrollWidth - titleBox!.clientWidth));
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(titleBox);
    return () => ro.disconnect();
  });
</script>

{#if np}
  <div
    class="mini"
    class:dragging
    style:--art-accent={art.accent ?? 'var(--accent)'}
    style:--share={share}
    style:transform="translateX({dx}px)"
    bind:this={el}
    role="button"
    tabindex="0"
    aria-label="Open the player: {np.name}"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={() => ((from = null), (dragging = false), (dx = 0))}
    onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), openPlayer())}
  >
    <div class="cover-wrap">
      {#if src}
        {#key src}<img class="cover" {src} alt="" draggable="false" onerror={() => coverFailed(src)} />{/key}
      {/if}
      <button class="own play" aria-label={paused() ? 'Play' : 'Pause'} onclick={() => send('play-pause')}
        ><Icon name={paused() ? 'play' : 'pause'} size={32} /></button
      >
    </div>
    <div class="body">
      <div class="fill" aria-hidden="true"></div>
      <div class="text">
        <div class="title" bind:this={titleBox}>
          <span bind:this={titleText} class:scroll={overflow > 0} style:--overflow="{overflow}px">{np.name}</span>
        </div>
        <div class="sub">
          <span class="artist">{np.artist ?? ''}</span>
          {#if np.progress}<span class="time">{clockText(pos)} / {dur ? clockText(dur) : '–:––'}</span>{/if}
        </div>
      </div>
      {#if np.favorite !== null}
        <button class="own fav" class:on={favorite()} disabled={live.pending} aria-label={favorite() ? 'Favourite' : 'Not a favourite'} aria-pressed={favorite()} onclick={toggleFavorite}
          ><Icon name="heart" size={28} /></button
        >
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Finamp's bar: 64 px high, 12 px from the edges, corners 12 px, a soft shadow; the cover square at the start. */
  .mini {
    position: relative;
    display: flex;
    height: 64px;
    overflow: hidden;
    border-radius: 12px;
    background: var(--surface);
    box-shadow: 0 4px 20px rgb(0 0 0 / 0.45);
    cursor: pointer;
    touch-action: pan-y;
    user-select: none;
    transition: transform 200ms ease, --art-accent 1s;
  }

  .mini.dragging {
    transition: --art-accent 1s;
  }

  .cover-wrap {
    position: relative;
    flex: none;
    width: 64px;
    height: 64px;
  }

  .cover {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    background: var(--surface);
  }

  /* Play/pause on the cover: a white icon with a dark shadow. */
  .play {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 0;
    background: rgb(0 0 0 / 0.15);
    border: 0;
    color: #fff;
    cursor: pointer;
  }

  .play :global(svg) {
    filter: drop-shadow(0 0 5px rgb(0 0 0 / 0.8));
  }

  /* The text area fills from the left as the track plays: the played part in the cover's colour, the rest that
     colour darkened (Finamp blends it at 75 % over black; light theme over white). */
  .body {
    position: relative;
    display: flex;
    flex: 1;
    align-items: center;
    min-width: 0;
    background: color-mix(in srgb, var(--art-accent) 25%, var(--bg));
  }

  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    width: calc(var(--share) * 100%);
    border-radius: 0 12px 12px 0;
    background: color-mix(in srgb, var(--art-accent) 55%, var(--bg));
    transition: width 250ms linear;
  }

  .text {
    position: relative;
    flex: 1;
    min-width: 0;
    padding: 0 8px 0 12px;
  }

  .title {
    overflow: hidden;
    font-size: 0.92rem;
    font-weight: 600;
    white-space: nowrap;
  }

  .title span {
    display: inline-block;
  }

  .title span.scroll {
    animation: marquee 12s linear infinite;
  }

  @keyframes marquee {
    0%,
    15% {
      transform: translateX(0);
    }
    70%,
    85% {
      transform: translateX(calc(-1 * var(--overflow)));
    }
    100% {
      transform: translateX(0);
    }
  }

  .sub {
    display: flex;
    gap: 8px;
    margin-top: 2px;
    color: color-mix(in srgb, var(--text) 80%, transparent);
    font-size: 0.82rem;
    white-space: nowrap;
  }

  .artist {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .time {
    flex: none;
    font-variant-numeric: tabular-nums;
  }

  .fav {
    position: relative;
    display: grid;
    flex: none;
    place-items: center;
    width: 48px;
    height: 64px;
    padding: 0;
    background: none;
    border: 0;
    color: var(--text);
    cursor: pointer;
  }

  .fav.on :global(svg) {
    fill: currentColor;
  }

  .fav:disabled {
    opacity: 0.5;
  }

  @media (prefers-reduced-motion: reduce) {
    .title span.scroll {
      animation: none;
    }
  }
</style>
