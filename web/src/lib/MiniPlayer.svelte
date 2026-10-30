<script lang="ts">
  import StartStopSheet from './StartStopSheet.svelte';
  import { port } from './player/port.svelte';
  // The mini-player along the bottom (decision 0018), as Finamp's now-playing bar: a floating bar with the cover and
  // play/pause on it, the title (scrolling when it does not fit), artist and time, and the heart. The whole bar fills
  // from the left as the track plays, in the cover's colour. Tap or swipe up: the full player (beside the page when
  // there is room, else over it). Swipe left or right: next or previous; the bar follows the finger and springs
  // back. Swipe down does nothing (Finamp stops and clears the queue; not on a remote across the room). The title does
  // not scroll (Finamp's default).
  // Where it plays sits on the bar, as Spotify's device button beside the heart (the owner, 2026-10-17): it opens the
  // output's sheet, and is in the accent while the music plays on another player. The pills at the top give it up
  // while the bar is there (lib/TopPill). Left of it, where the bar has room, the queue (the owner, 2026-10-24): it opens
  // the queue's sheet, and answers tracks put in from elsewhere (lib/queueCue): list-start a moment for Play next,
  // list-end for Add to queue.
  import { tick } from 'svelte';
  import { BANDCAMP } from './bandcampPreview';
  import { portal } from './portal';
  import Icon from './Icon.svelte';
  import OutputSheet from './OutputSheet.svelte';
  import QueueSheet from './QueueSheet.svelte';
  import { queueCue } from './queueCue.svelte';
  import { playingCrateOuts } from './crates.svelte';
  import HeartSheet from './HeartSheet.svelte';
  import { longpress } from './longpress';
  import { player } from './live.svelte';
  import { panel } from './panel.svelte';
  import { prefs } from './prefs.svelte';
  import Cover from './Cover.svelte';
  import { art, clockText, coverFailed, coverSrc, shownCover, duration, favorite, paused, pending, playing, shownPlaying, position, seekTo, send, toggleFavorite, playPause, rampInert, shownPaused, waveStill, playingKey } from './remote.svelte';
  import SeekWave from './SeekWave.svelte';
  import { Seek } from './seekState.svelte';
  import { askedOf, playerShown } from './following.svelte';
  import { loadWaveform, waveformOf, waveLoading } from './waveforms.svelte';

  // As shown: the new track's words with its cover, one commit (lib/remote shownPlaying).
  const np = $derived(shownPlaying());
  const src = $derived(shownCover());
  // A long press on the heart: the full player's sheet (lib/HeartSheet), its crate Excludes included.
  let heartMenu = $state(false);
  const dur = $derived(duration());
  // A long press anywhere but on a button grabs the waveform (the owner, 2026-10-26: as the full player's,
  // lib/SeekWave): the bar is the waveform alone while held, its usual self again once let go.
  const seek = new Seek(position, () => dur, seekTo);
  const pos = $derived(seek.pos);
  const canSeek = $derived(port().seekable && !!dur);
  $effect(() => {
    if (np) loadWaveform(np.source, np.id);
  });
  const wave = $derived(np ? waveformOf(np.source, np.id) : null);
  let grabbing = $state(false);
  /** The extra long press under way: the bar's words and buttons fade out over its second (the owner, 2026-10-26). */
  let arming = $state(false);
  let seekWave = $state<ReturnType<typeof SeekWave>>();
  let grabTimer: ReturnType<typeof setTimeout> | undefined;
  /** Where the finger is, for the grab. */
  let finger = { pointerId: 0, clientX: 0 };
  /** An extra long press (the owner, 2026-10-26): the bar is swiped too, so twice a menu's long press, held still. */
  const GRAB_MS = 1000;
  $effect(() => () => clearTimeout(grabTimer));
  const share = $derived(dur ? Math.min(1, pos / dur) : 0);
  let outputOpen = $state(false);
  /** The bar's width from which the queue button has room beside the title. */
  const QUEUE_ROOM = 420;
  /** How long the queue button shows what went in. */
  const ANSWER_MS = 1400;
  let queueOpen = $state(false);
  /** The bar's width: the queue button shows where there is room for it beside the title. */
  let barWidth = $state(0);
  const roomForQueue = $derived(barWidth >= QUEUE_ROOM);
  /** What the queue button answers now: a Play next or an Add to queue a moment ago, else nothing. */
  let answer = $state<'next' | 'last' | null>(null);
  let answered: ReturnType<typeof setTimeout> | undefined;
  let seen = queueCue.n;
  $effect(() => {
    if (queueCue.n === seen) return;
    seen = queueCue.n;
    answer = queueCue.how;
    clearTimeout(answered);
    answered = setTimeout(() => (answer = null), ANSWER_MS);
  });

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
    if (grabbing) return;
    if ((e.target as Element).closest('.own')) return; // the play button, the device and the heart act on their own
    from = { x: e.clientX, y: e.clientY, id: e.pointerId, t: e.timeStamp };
    last = { x: e.clientX, t: e.timeStamp };
    speed = 0;
    moved = false;
    finger = { pointerId: e.pointerId, clientX: e.clientX };
    clearTimeout(grabTimer);
    // Anywhere but on a button (those return above), the words included (the owner, 2026-10-26).
    if (canSeek) {
      grabTimer = setTimeout(grab, GRAB_MS);
      arming = true;
    }
  }
  /** Held still long enough: the waveform takes the bar and the finger (the swipe and the tap are off). */
  async function grab() {
    arming = false;
    if (!from || moved) return;
    from = null;
    grabbing = true;
    navigator.vibrate?.(8);
    await tick();
    seekWave?.grab(finger);
  }
  function move(e: PointerEvent) {
    if (!from || e.pointerId !== from.id) return;
    finger = { pointerId: e.pointerId, clientX: e.clientX };
    const [mx, my] = [e.clientX - from.x, e.clientY - from.y];
    if (Math.abs(mx) > 8 || Math.abs(my) > 8) (clearTimeout(grabTimer), (arming = false));
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
    clearTimeout(grabTimer);
    arming = false;
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

  /** A long press on the play button: Start and stop's controls (lib/StartStopSheet; the owner, 2026-10-26). */
  let startStopOpen = $state(false);
</script>

{#if np}
  <div
    class="mini"
    class:dragging
    class:arming
    class:grabbing
    style:--art-accent={(prefs.coverColourMini && art.accent) || 'var(--accent)'}
    style:--share={share}
    style:transform="translateX({dx}px)"
    bind:this={el}
    bind:clientWidth={barWidth}
    role="button"
    tabindex="0"
    aria-label="Open the player: {np.name}"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={() => (clearTimeout(grabTimer), (arming = false), (from = null), (dragging = false), (dx = 0))}
    onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), openPlayer())}
  >
    <div class="cover-wrap">
      <!-- the stand-in only when there is no cover to load (lib/Cover), not while one loads -->
      {#if src || !coverSrc()}
        {#key src}<Cover {src} lazy={false} onfail={coverFailed} />{/key}
      {/if}
      <button class="own play" aria-label={shownPaused() ? 'Play' : 'Pause'} aria-disabled={rampInert()} onclick={playPause} use:longpress={() => (startStopOpen = true)}
        ><Icon name={shownPaused() ? 'play' : 'pause'} size={32} /></button
      >
    </div>
    <div class="body">
      {#if grabbing}
        <!-- The waveform alone while it is held (lib/SeekWave), the rest of the bar back once let go. -->
        <div class="grab">
          <SeekWave bind:this={seekWave} {seek} bins={wave} loading={waveLoading(np.source, np.id)} {dur} {canSeek} paused={waveStill()} room={el} track={playingKey(playing())} onend={() => (grabbing = false)} />
        </div>
      {:else}
      <div class="fill" aria-hidden="true"></div>
      <div class="text">
        <div class="title">{np.name}</div>
        <div class="sub">
          <span class="artist">{np.artist ?? ''}</span>
          {#if np.progress}<span class="time">{clockText(pos)} / {dur ? clockText(dur) : '–:––'}</span>{/if}
        </div>
      </div>
      {#if roomForQueue}
        <button class="own queue" class:answer={!!answer} aria-haspopup="dialog" aria-label="Queue" onclick={() => (queueOpen = true)}
          >{#key answer}<Icon name={answer === 'next' ? 'listStart' : answer === 'last' ? 'listEnd' : 'playlists'} size={24} />{/key}{#if askedOf(playerShown())}<span class="asked-count"
              >{askedOf(playerShown())}</span
            >{/if}</button
        >
      {/if}
      <button class="own device" class:elsewhere={!!np.player} aria-haspopup="dialog"
        aria-label={np.player ? `Playing on ${np.player}; play on` : 'Where it plays'} onclick={() => (outputOpen = true)}
        ><Icon name="speaker" size={24} /></button
      >
      <!-- No heart for the device's own music (local mode, decision 0052); a Bandcamp track's crossed out, said so. -->
      {#if np.source === BANDCAMP}
        <span class="own fav off" role="img" aria-label="On Bandcamp" title="On Bandcamp"><Icon name="heartOff" size={28} /></span>
      {:else if np.favorite !== null && np.source !== 'device'}
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
      {/if}
    </div>
  </div>
{/if}
{#if outputOpen}
  <OutputSheet onclose={() => (outputOpen = false)} />
{/if}
{#if queueOpen && np}
  <!-- At the page's end, not in the mini-player's dock (its own layer order): over the bar and the floating controls,
       as the other sheets (the owner, 2026-10-25). -->
  <div use:portal><QueueSheet {src} screen onclose={() => (queueOpen = false)} /></div>
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

{#if startStopOpen}<StartStopSheet onclose={() => (startStopOpen = false)} />{/if}

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
    /* Every swipe the bar's own (the owner, 2026-10-26): with pan-y the browser took the upward ones to scroll and
       cancelled them, so Swipe up never opened the player; and a hold drifting a little would lose its grab. */
    touch-action: none;
    user-select: none;
    -webkit-touch-callout: none;
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

  /* The extra long press under way: the words and buttons fade out over its second, after a moment (a tap does not
     flicker); let go or swiped, they are back at once. */
  .body > :global(*) {
    transition: opacity 120ms ease-out;
  }

  .mini.arming .body > :global(*) {
    opacity: 0;
    transition: opacity 850ms linear 150ms;
  }

  /* Held, the waveform's lens stands over the bar, outside it (the owner, 2026-10-26: it was cut off): nothing is
     clipped, the cover and the bar keeping the rounded corners themselves. */
  .mini.grabbing {
    overflow: visible;
  }

  .mini.grabbing .cover-wrap {
    overflow: hidden;
    border-radius: 12px 0 0 12px;
  }

  .mini.grabbing .body {
    border-radius: 0 12px 12px 0;
  }

  /* The waveform alone across the bar while it is held (lib/SeekWave), clear of the cover. */
  .grab {
    display: flex;
    flex: 1;
    align-items: center;
    min-width: 0;
    padding: 0 14px;
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
  .queue,
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

  .queue,
  .device {
    width: 40px;
  }

  /* Tracks went in: the queue's answer, in the accent, with a small pop. */
  .queue.answer {
    color: var(--accent-ink);
  }

  .queue.answer :global(svg) {
    animation: pop 0.35s ease-out;
  }

  @keyframes pop {
    0% {
      transform: scale(1);
    }
    40% {
      transform: scale(1.25);
    }
    100% {
      transform: scale(1);
    }
  }

  .device.elsewhere {
    color: var(--accent-ink);
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

    .queue.answer :global(svg) {
      animation: none;
    }
  }

  /* Animations off on this device (Display): the same as the system's reduced motion. */
  :global(:root[data-motion='off']) .mini {
    transition: none;
  }

  :global(:root[data-motion='off']) .queue.answer :global(svg) {
    animation: none;
  }

  .fav.off {
    color: var(--muted);
    cursor: default;
  }

  /* Tracks asked for of this player (0.10): how many, on the queue button's corner. */
  .asked-count {
    position: absolute;
    top: 12px;
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
</style>
