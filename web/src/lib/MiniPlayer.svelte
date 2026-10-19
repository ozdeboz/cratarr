<script lang="ts">
  // The mini-player along the bottom (decision 0018), as Finamp's now-playing bar: a floating bar with the cover and
  // play/pause on it, the title (scrolling when it does not fit), artist and time, and the heart. The whole bar fills
  // from the left as the track plays, in the cover's colour. Tap or swipe up: the full player (beside the page when
  // there is room, else over it). Swipe left or right: next or previous; the bar follows the finger and springs
  // back. Swipe down does nothing (Finamp stops and clears the queue; not on a remote across the room). The title does
  // not scroll (Finamp's default).
  // Where it plays sits on the bar, as Spotify's device button beside the heart (the owner, 2026-10-17): it opens the
  // output's sheet, and is in the accent while the music plays on another player. The pills at the top give it up
  // while the bar is there (lib/TopPill).
  import Icon from './Icon.svelte';
  import OutputSheet from './OutputSheet.svelte';
  import { playingCrateOuts } from './crates.svelte';
  import HeartSheet from './HeartSheet.svelte';
  import { longpress } from './longpress';
  import { player } from './live.svelte';
  import { panel } from './panel.svelte';
  import { prefs } from './prefs.svelte';
  import { art, clockText, coverFailed, shownCover, duration, favorite, paused, pending, playing, position, send, toggleFavorite } from './remote.svelte';

  const np = $derived(playing());
  const src = $derived(shownCover());
  // A long press on the heart: the full player's sheet (lib/HeartSheet), its crate Excludes included.
  let heartMenu = $state(false);
  const pos = $derived(position());
  const dur = $derived(duration());
  const share = $derived(dur ? Math.min(1, pos / dur) : 0);
  let outputOpen = $state(false);

  /** The full player: beside the page when the window has room and that is the chosen place, else over it. */
  // Tapping it brings the player: into the panel beside the page (over a viewed playlist there), or over everything.
  function openPlayer() {
    if (player.roomy && prefs.sidebar) {
      player.min = false;
      panel.playerTop = true;
    } else player.full = true;
  }

  // Swipes, as Finamp's Dismissible: horizontal ones move the bar with the finger and skip past 40 % of its width or on
  // a flick (700 px/s), springing back in 200 ms; upward ones open the player past 15 % of its height; a tap opens
  // it too.
  let dx = $state(0);
  let dragging = $state(false);
  let from: { x: number; y: number; id: number; t: number } | null = null;
  let last: { x: number; t: number } = { x: 0, t: 0 };
  let speed = 0; // px per ms, horizontal, from the last two moves
  let moved = false;
  let el = $state<HTMLElement>();

  function down(e: PointerEvent) {
    if ((e.target as Element).closest('.own')) return; // the play button, the device and the heart act on their own
    from = { x: e.clientX, y: e.clientY, id: e.pointerId, t: e.timeStamp };
    last = { x: e.clientX, t: e.timeStamp };
    speed = 0;
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
    if (e.timeStamp > last.t) speed = (e.clientX - last.x) / (e.timeStamp - last.t);
    last = { x: e.clientX, t: e.timeStamp };
    if (Math.abs(mx) > 8 || Math.abs(my) > 8) moved = true;
  }
  /** Opened by a tap or a swipe: the bar is gone at once, so the click that follows the same tap would land on what
   *  lay under it (a playlist's chip, ticking it in Select). That one click is swallowed. */
  function swallowTheTapsClick() {
    const stop = (e: Event) => {
      e.stopPropagation();
      e.preventDefault();
    };
    addEventListener('click', stop, { capture: true, once: true });
    setTimeout(() => removeEventListener('click', stop, { capture: true }), 500);
  }

  function up(e: PointerEvent) {
    if (!from || e.pointerId !== from.id) return;
    const [mx, my] = [e.clientX - from.x, e.clientY - from.y];
    const width = el?.clientWidth ?? 300;
    from = null;
    const flick = Math.abs(speed) > 0.7 && Math.sign(speed) === Math.sign(mx);
    if (dragging && (Math.abs(mx) > width * 0.4 || flick)) send(mx < 0 ? 'next' : 'previous');
    else if ((!dragging && my < -64 * 0.15 && -my > Math.abs(mx)) || !moved) {
      swallowTheTapsClick();
      // Opened by a finger or the mouse, it keeps no focus: none to show when the player closes again.
      el?.blur();
      openPlayer();
    }
    dragging = false;
    dx = 0;
  }

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
        <div class="title">{np.name}</div>
        <div class="sub">
          <span class="artist">{np.artist ?? ''}</span>
          {#if np.progress}<span class="time">{clockText(pos)} / {dur ? clockText(dur) : '–:––'}</span>{/if}
        </div>
      </div>
      <button class="own device" class:elsewhere={!!np.player} aria-haspopup="dialog"
        aria-label={np.player ? `Playing on ${np.player}; play on` : 'Where it plays'} onclick={() => (outputOpen = true)}
        ><Icon name="speaker" size={24} /></button
      >
      <!-- No heart for the device's own music (local mode, decision 0052). -->
      {#if np.favorite !== null && np.source !== 'device'}
        <button
          class="own fav"
          class:on={favorite()}
          disabled={pending()}
          aria-label={favorite() ? 'Favourite' : 'Not a favourite'}
          aria-pressed={favorite()}
          onclick={toggleFavorite}
          use:longpress={() => (heartMenu = true)}><Icon name="heart" size={28} /></button
        >
      {/if}
    </div>
  </div>
{/if}
{#if outputOpen}
  <OutputSheet onclose={() => (outputOpen = false)} />
{/if}
{#if heartMenu && np}
  <HeartSheet
    track={np}
    cover={src}
    favourite={{ on: favorite(), disabled: pending() || np.favorite === null, flip: toggleFavorite }}
    outs={playingCrateOuts()}
    onclose={() => (heartMenu = false)}
  />
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
    transition: transform 200ms linear, --art-accent 1s linear;
  }

  /* Focus shows only from the keyboard, as a ring in the cover's colour (never the browser's black one). */
  .mini:focus {
    outline: none;
  }

  .mini:focus-visible {
    outline: 2px solid var(--art-accent, var(--accent));
    outline-offset: 2px;
  }

  .mini.dragging {
    transition: --art-accent 1s linear;
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
  }

  .text {
    position: relative;
    flex: 1;
    min-width: 0;
    padding: 0 8px 0 12px;
  }

  /* One line, cut off with an ellipsis: Finamp's default (its scrolling title is an option, off by default). */
  .title {
    overflow: hidden;
    font-size: 0.92rem;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  .fav,
  .device {
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

  .device {
    width: 40px;
  }

  .device.elsewhere {
    color: var(--accent);
  }

  .fav.on :global(svg) {
    fill: currentColor;
  }

  .fav:disabled {
    opacity: 0.5;
  }

  @media (prefers-reduced-motion: reduce) {
    .mini {
      transition: none;
    }
  }
</style>
