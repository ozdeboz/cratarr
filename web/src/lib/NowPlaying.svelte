<script lang="ts">
  // The full player (decision 0018): over the page (`overlay`, opened from the mini-player) or beside it (`side`, the
  // shell's sidebar on wide windows). It shows the cover, the track, the controls, the progress, the favourite and
  // how many playlists hold the track, laid out by lib/playerLayout.ts, on the blurred cover in its colours.
  import { untrack } from 'svelte';
  import { linear } from 'svelte/easing';
  import { fade } from 'svelte/transition';
  import { reducedMotion } from './motion';
  import Icon from './Icon.svelte';
  import { player, refresh } from './live.svelte';
  import TrackMenu from './TrackMenu.svelte';
  import QueueSheet from './QueueSheet.svelte';
  import OutputSheet from './OutputSheet.svelte';
  import SleepSheet from './SleepSheet.svelte';
  import { sleep, sleepLeft } from './sleep.svelte';
  import { fit, STEPS, type Fit } from './playerLayout';
  import { prefs } from './prefs.svelte';
  import { art, clockText, coverFailed, shownCover, duration, favorite, paused, pending, playing, position, seekTo, send, toggleFavorite } from './remote.svelte';
  import { isLocal, shownPlaylists } from './output.svelte';
  import { local, setRadio, setRepeat, setShuffle } from './localPlayer.svelte';
  import { keepViewed, panel } from './panel.svelte';

  let { place }: { place: 'overlay' | 'side' } = $props();

  const np = $derived(playing());
  const memberCount = $derived(shownPlaylists().filter((p) => p.member).length);
  const src = $derived(shownCover());
  // The backdrop list: the current blurred cover, and while it fades in, the one before it.
  let backdrops = $state<{ key: number; src: string }[]>([]);
  let backdropKey = 0;
  $effect(() => {
    const s = src;
    untrack(() => {
      if (!s) backdrops = [];
      else if (backdrops.at(-1)?.src !== s) backdrops = [{ key: ++backdropKey, src: s }];
    });
  });
  // Finamp's cover: a hard cut when the image is there at once, which it always is here (remote.svelte.ts shows a
  // cover once it has loaded), so it never fades in.
  const dur = $derived(duration());

  // Seeking (decision 0019, now that Cratarr plays music itself): as Finamp's slider, the handle and the time follow
  // the finger and the player seeks only on release. Only on this device for now: seeking the server's player is
  // tried on it first.
  const canSeek = $derived(isLocal() && !!dur);
  /** Where the handle is held (seconds), and after release until the player is there. */
  let held = $state<number | null>(null);
  let heldTimer: ReturnType<typeof setTimeout> | undefined;
  const pos = $derived(held ?? position());
  const at = (e: PointerEvent, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * (dur ?? 0);
  };
  function seekDown(e: PointerEvent) {
    if (!canSeek) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    clearTimeout(heldTimer);
    held = at(e, el);
  }
  function seekMove(e: PointerEvent) {
    if (held === null || !(e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId)) return;
    held = at(e, e.currentTarget as HTMLElement);
  }
  function seekUp(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    if (held === null || !el.hasPointerCapture(e.pointerId)) return;
    release(at(e, el));
  }
  /** Seek, and keep the handle there until the player has moved (no jump back to the old position). */
  function release(secs: number) {
    held = secs;
    seekTo(secs);
    heldTimer = setTimeout(() => (held = null), 600);
  }
  function seekKey(e: KeyboardEvent) {
    if (!canSeek || !dur) return;
    const now = position();
    const to =
      e.key === 'ArrowLeft' ? now - 5 : e.key === 'ArrowRight' ? now + 5 : e.key === 'Home' ? 0 : e.key === 'End' ? dur - 1 : null;
    if (to === null) return;
    e.preventDefault();
    release(Math.min(dur, Math.max(0, to)));
  }

  // Info chips (decision 0019): what the server knows about the file and its plays, read-only.
  const chips = $derived.by(() => {
    const i = np?.info;
    if (!i) return [];
    const out: string[] = [];
    if (i.explicit) out.push('Explicit');
    if (i.play_count !== null) out.push(`${i.play_count} ${i.play_count === 1 ? 'play' : 'plays'}`);
    const codec = [i.codec?.toUpperCase(), i.bitrate_kbps ? `${i.bitrate_kbps} kbps` : null].filter(Boolean).join(' @ ');
    if (codec) out.push(codec);
    const format = [i.bit_depth ? `${i.bit_depth}-bit` : null, i.sample_rate_hz ? `${+(i.sample_rate_hz / 1000).toFixed(1)} kHz` : null]
      .filter(Boolean)
      .join(' · ');
    if (format) out.push(format);
    return out;
  });

  // The track menu (the menu button, or a right-click on the cover) and the queue (decision 0019).
  let menuAt = $state<DOMRect | null>(null);
  // The menu's entries, or straight to its playlists (the top bar's playlists button).
  let menuStart = $state<'menu' | 'lists'>('menu');
  let queueOpen = $state(false);
  let outputOpen = $state(false);
  let sleepOpen = $state(false);
  const openMenu = (e: Event, start: 'menu' | 'lists' = 'menu') => {
    menuStart = start;
    menuAt = (e.currentTarget as HTMLElement).getBoundingClientRect();
  };
  // The track's playlists (the top bar's button says it to a screen reader) and its place in the queue (the bottom row's
  // one label).
  const inPlaylists = $derived(memberCount === 0 ? 'In no playlist' : `In ${memberCount} ${memberCount === 1 ? 'playlist' : 'playlists'}`);
  const queuePlace = $derived.by(() => {
    if (isLocal()) return local.index < 0 ? '' : `${local.index + 1} of ${local.queue.length}`;
    const np = playing();
    return np ? `1 of ${np.upcoming.length + 1}` : '';
  });
  // Closing: over the page the chevron, Esc and a swipe down close it; beside the page they fold it to the
  // mini-player until that is tapped (not remembered). Maximise puts it over the page, the sidebar button back beside
  // it; both are remembered (prefs.sidebar).
  function close() {
    // Beside the page over a viewed playlist: folding shows the playlist again (decision 0023).
    if (place === 'side' && panel.viewed !== null) panel.playerTop = false;
    else if (place === 'side') player.min = true;
    else player.full = false;
  }
  function maximise() {
    prefs.sidebar = false;
    player.full = true;
  }
  // Beside the page again: the player moves into the panel, unless a playlist or crate is viewed there: that keeps the
  // panel, and the mini-player's tap brings the player over it.
  function toSidebar() {
    prefs.sidebar = true;
    player.min = false;
    panel.playerTop = false;
    player.full = false;
  }
  $effect(() => {
    if (place !== 'overlay') return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || menuAt) return;
      if (queueOpen) queueOpen = false;
      else close();
    };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  });

  /** The Playlists page, from the player over the page (its playlists' sheet): the player closes. */
  function toPlaylistsPage() {
    player.full = false;
    location.hash = keepViewed('#/');
  }

  // Gestures, over the page and beside it. Anywhere: swipe down closes, swipe up opens the queue. On the cover: swipe left or right skips, a
  // tap plays or pauses, a double-tap toggles the favourite. A swipe never also counts as a tap.
  let head = $state<HTMLElement>();
  let swiped = false;
  $effect(() => {
    if (!head) return;
    let from: { x: number; y: number; cover: boolean } | null = null;
    const down = (e: PointerEvent) => {
      // The seek bar's drags are its own.
      if ((e.target as Element).closest?.('.seek')) return;
      from = { x: e.clientX, y: e.clientY, cover: !!(e.target as Element).closest?.('.cover-btn') };
    };
    const up = (e: PointerEvent) => {
      if (!from) return;
      const [dx, dy] = [e.clientX - from.x, e.clientY - from.y];
      const onCover = from.cover;
      from = null;
      if (onCover && Math.abs(dx) > 60 && Math.abs(dx) > 2 * Math.abs(dy)) {
        swiped = true;
        send(dx < 0 ? 'next' : 'previous');
      } else if (dy > 80 && dy > 2 * Math.abs(dx)) {
        swiped = true;
        close();
      } else if (dy < -80 && -dy > 2 * Math.abs(dx)) {
        swiped = true;
        if (playing()) queueOpen = true;
      }
    };
    const el = head;
    el.addEventListener('pointerdown', down);
    el.addEventListener('pointerup', up);
    return () => {
      el.removeEventListener('pointerdown', down);
      el.removeEventListener('pointerup', up);
    };
  });

  /** A tap waits this long for a second one (double-tap: favourite) before it plays or pauses. */
  const DOUBLE_TAP_MS = 250;
  let tapTimer: ReturnType<typeof setTimeout> | null = null;
  function coverTap() {
    if (swiped) {
      swiped = false;
      return;
    }
    if (tapTimer) {
      clearTimeout(tapTimer);
      tapTimer = null;
      toggleFavorite();
      return;
    }
    tapTimer = setTimeout(() => {
      tapTimer = null;
      send('play-pause');
    }, DOUBLE_TAP_MS);
  }

  // Layout (lib/playerLayout.ts): on every resize, and when what is shown changes, the player measures its controls
  // with steps dropped one by one and settles on what gives way, the cover's size and the orientation.
  let controls = $state<HTMLElement>();
  let layout = $state<Fit | null>(null);
  const dropped = (step: (typeof STEPS)[number]) => !!layout && layout.level > STEPS.indexOf(step);

  function relayout() {
    const el = head;
    const box = controls;
    if (!el || !box) return;
    const cs = getComputedStyle(el);
    const width = el.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    const height = el.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom);
    if (width <= 0 || height <= 0) return;
    // Measuring sets the classes and the box's width directly (its height free) and puts them back afterwards; the
    // state then sets the outcome.
    const before = { cls: el.className, width: box.style.width, height: box.style.height };
    const measure = (landscape: boolean, level: number, controlsWidth: number) => {
      el.classList.toggle('landscape', landscape);
      STEPS.forEach((step, i) => el.classList.toggle(`drop-${step}`, level > i));
      box.style.width = `${controlsWidth}px`;
      box.style.height = 'auto';
      return box.getBoundingClientRect().height;
    };
    const next = fit(width, height, measure);
    el.className = before.cls;
    box.style.width = before.width;
    box.style.height = before.height;
    if (JSON.stringify(next) !== JSON.stringify(layout)) layout = next;
  }

  $effect(() => {
    if (!head) return;
    const ro = new ResizeObserver(() => relayout());
    ro.observe(head);
    return () => ro.disconnect();
  });
  // What is shown changes the controls' height (a long title, the pill, a progress bar or none).
  $effect(() => {
    void [np?.id, np?.name, np?.artist, np?.album, np?.player, !!np?.progress, prefs.showProgress, chips.length];
    untrack(() => requestAnimationFrame(relayout));
  });
</script>

<div class="player" class:in-sidebar={place === 'side'} style:--art-accent={art.accent ?? 'var(--accent)'}>
  <!-- The blurred cover: a new one fades in over the old one in a second (Finamp's background), the old one staying
       until it is covered. -->
  {#each backdrops as b (b.key)}
    <img class="backdrop" src={b.src} alt="" aria-hidden="true" in:fade={{ duration: reducedMotion() ? 0 : 1000, easing: linear }} out:fade={{ duration: 0, delay: reducedMotion() ? 0 : 1000 }} />
  {/each}
  <div class="veil"></div>
  <header
    class:landscape={layout?.landscape}
    class:drop-gapsBig={dropped('gapsBig')}
    class:drop-buttons={dropped('buttons')}
    class:drop-gapsSmall={dropped('gapsSmall')}
    class:drop-chips={dropped('chips')}
    class:drop-title={dropped('title')}
    class:drop-progress={dropped('progress')}
    bind:this={head}
  >
    <button class="corner left" aria-label={place === 'side' ? 'Fold the player' : 'Close the player'} onclick={close}><Icon name="collapse" size={26} /></button>
    <!-- Top right, icons only (what is on shows lit): the track's playlists (lit while it is in one), the sleep timer
         (only where the page holds the queue; lit while one runs), and where the player goes. -->
    <div class="corners">
      {#if np}
        <button class="corner" class:on={memberCount > 0} aria-label="Add to playlist; {inPlaylists}" aria-haspopup="menu" onclick={(e) => openMenu(e, 'lists')}
          ><Icon name="listAdd" size={22} /></button
        >
      {/if}
      {#if np && isLocal()}
        <button
          class="corner sleep"
          class:on={sleep() !== null}
          aria-label={sleep() ? `Sleep timer, ${sleepLeft()}` : 'Sleep timer'}
          aria-haspopup="dialog"
          onclick={() => (sleepOpen = true)}><Icon name="moon" size={22} /></button
        >
      {/if}
      {#if place === 'side'}
        <button class="corner" aria-label="Over the page" onclick={maximise}><Icon name="expand" size={20} /></button>
      {:else if player.roomy}
        <button class="corner" aria-label="Beside the page" onclick={toSidebar}><Icon name="sidebar" size={22} /></button>
      {/if}
    </div>
    <!-- Finamp's layout: the cover's region, then the controls' box with three groups spread in it (track name,
         controls, actions); in landscape side by side. Sizes from lib/playerLayout.ts. -->
    <div class="album" style:width={layout ? `${layout.album.width}px` : null} style:height={layout ? `${layout.album.height}px` : null}>
      {#if np && src}
        {#key src}
          <button
            class="cover-btn"
            style:width={layout ? `${layout.cover}px` : null}
            style:height={layout ? `${layout.cover}px` : null}
            aria-label="Play or pause"
            tabindex="-1"
            onclick={coverTap}
            oncontextmenu={(e) => (e.preventDefault(), openMenu(e))}
          >
            <img class="cover" {src} alt="" draggable="false" onerror={() => coverFailed(src)} />
          </button>
        {/key}
      {/if}
    </div>
    <div
      class="controls"
      bind:this={controls}
      style:width={layout ? `${layout.controls.width}px` : null}
      style:height={layout ? `${layout.controls.height}px` : null}
    >
      <i class="sp a"></i>
      <div class="track">
        {#if np}
          <!-- Where it plays, and where to play it: the output's sheet. Always there (never gives way). -->
          <button class="pill" aria-haspopup="dialog" aria-label="Playing on {np.player ?? 'another player'}; play on" onclick={() => (outputOpen = true)}
            ><Icon name="speaker" size={14} />{#if np.player}<span>{np.player}</span>{/if}</button
          >
          <div class="title">{np.name}</div>
          <div class="sub">{[np.artist, np.album].filter(Boolean).join(' · ')}</div>
        {:else}
          <div class="title idle">Nothing is playing</div>
        {/if}
      </div>
      {#if np}
        <i class="sp b"></i>
        <div class="area">
          {#if chips.length}
            <div class="chips" aria-label="About this track">
              {#each chips as c, i (i)}<span>{c}</span>{/each}
            </div>
          {/if}
          {#if np.progress && prefs.showProgress}
            <div
              class="progress"
              class:paused={np.progress.paused}
              class:estimated={np.progress.estimated}
              title={np.progress.estimated
                ? 'Estimated: no player reports this track, so this counts from when it started'
                : np.progress.paused
                  ? 'Paused'
                  : undefined}
            >
              <span class="t">{np.progress.estimated ? '≈' : ''}{clockText(pos)}</span>
              <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
              <div
                class="seek"
                class:can={canSeek}
                class:held={held !== null}
                role={canSeek ? 'slider' : 'progressbar'}
                tabindex={canSeek ? 0 : undefined}
                aria-label={np.progress.paused ? 'Track position, paused' : 'Track position'}
                aria-valuemin={0}
                aria-valuemax={dur ?? undefined}
                aria-valuenow={Math.floor(pos)}
                aria-valuetext={dur ? `${clockText(pos)} of ${clockText(dur)}` : clockText(pos)}
                onpointerdown={seekDown}
                onpointermove={seekMove}
                onpointerup={seekUp}
                onpointercancel={() => (held = null)}
                onkeydown={seekKey}
              >
                <div class="bar"><div class="fill" style:transform="scaleX({dur ? pos / dur : 0})"></div></div>
                {#if canSeek}<span class="thumb" style:left="{dur ? (pos / dur) * 100 : 0}%"></span>{/if}
              </div>
              <span class="t">{dur ? clockText(dur) : '–:––'}</span>
            </div>
          {/if}
          <div class="transport" role="group" aria-label="Player controls">
            <!-- Finamp's shuffle, left of previous; only where the page holds the queue (here, the app, a Cratarr
                 player). -->
            {#if isLocal()}
              <button class="shuffle" class:on={local.shuffled} aria-label="Shuffle" aria-pressed={local.shuffled} onclick={() => setShuffle(!local.shuffled)}
                ><Icon name="shuffle" size={22} /></button
              >
            {/if}
            <button aria-label="Previous track" onclick={() => send('previous')}><Icon name="previous" size={24} /></button>
            <button class="main" aria-label={paused() ? 'Play' : 'Pause'} onclick={() => send('play-pause')}
              ><Icon name={paused() ? 'play' : 'pause'} size={32} /></button
            >
            <button aria-label="Next track" onclick={() => send('next')}><Icon name="next" size={24} /></button>
            {#if isLocal()}
              <!-- Finamp's repeat, right of next: off, all, one. -->
              <button
                class="shuffle"
                class:on={local.repeat !== 'off'}
                aria-label={local.repeat === 'one' ? 'Repeat one' : local.repeat === 'all' ? 'Repeat all' : 'Repeat'}
                aria-pressed={local.repeat !== 'off'}
                onclick={() => setRepeat(local.repeat === 'off' ? 'all' : local.repeat === 'all' ? 'one' : 'off')}
                ><Icon name={local.repeat === 'one' ? 'repeatOne' : 'repeat'} size={22} /></button
              >
            {/if}
          </div>
        </div>
        <i class="sp c"></i>
        <div class="side">
          <button class="act" aria-label="Track menu" aria-haspopup="menu" onclick={openMenu}><Icon name="menu" size={24} /></button>
          <!-- Always there; until the server says whether it is a favourite, not to be pressed. -->
          <button
            class="fav"
            class:on={favorite()}
            disabled={pending() || np.favorite === null}
            aria-label={favorite() ? 'Favourite' : 'Not a favourite'}
            aria-pressed={favorite()}
            onclick={toggleFavorite}><Icon name="heart" size={26} /></button
          >
          <!-- Radio (decision 0036, amended), where the device button was; only where the page holds the queue. A tap switches it; lit while on. -->
          {#if isLocal()}
            <button class="act radio" class:on={local.radio} aria-label="Radio" aria-pressed={local.radio} onclick={() => setRadio(!local.radio)}
              ><Icon name="radio" size={24} /></button
            >
          {/if}
          <button class="act labelled" aria-label="Queue" aria-expanded={queueOpen} onclick={() => (queueOpen = true)}
            ><Icon name="playlists" size={24} /><small>{queuePlace}</small></button
          >
        </div>
      {/if}
      <i class="sp d"></i>
    </div>
  </header>

  {#if queueOpen && np}
    <QueueSheet src={src} onclose={() => (queueOpen = false)} />
  {/if}
</div>

{#if menuAt}
  <TrackMenu start={menuStart} onplaylists={place === 'overlay' ? toPlaylistsPage : null} onclose={() => (menuAt = null)} />
{/if}
{#if outputOpen}
  <OutputSheet onclose={() => (outputOpen = false)} />
{/if}
{#if sleepOpen}
  <SleepSheet onclose={() => (sleepOpen = false)} />
{/if}


<style>
  /* The player's own surface: the cover, heavily blurred, under a veil (dark, or light in the light theme), as
     Finamp's. Its accent (--art-accent, from the cover; lib/remote.svelte.ts) fades over a second between tracks. */
  .player {
    position: relative;
    display: flex;
    flex: 1 1 0;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: var(--bg);
    transition: --art-accent 1s linear;
  }

  @media (prefers-reduced-motion: reduce) {
    .player {
      transition: none;
    }
  }

  .backdrop {
    position: absolute;
    inset: -15%;
    width: 130%;
    height: 130%;
    object-fit: cover;
    filter: blur(60px) saturate(1.2);
    pointer-events: none;
  }

  .veil {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    pointer-events: none;
  }

  /* Finamp's player layout (lib/playerLayout.ts): portrait = the cover's region (the cover centred in it) over the
     controls' box; landscape = the cover's region on the left, the controls' box centred in the rest. Inside the box
     its three groups are spread evenly (Finamp's spaceEvenly; in landscape its spacers 4 / 4 / 10 / 4). What gives
     way (drop-*): the large gaps, the large play button, the player pill, the small gaps, the info chips, the title's
     second line, the progress bar. */
  header {
    box-sizing: border-box;
    position: relative;
    display: flex;
    flex: 1 1 0;
    flex-direction: column;
    align-items: center;
    min-height: 0;
    padding: 53px 0 env(safe-area-inset-bottom);
    overflow-y: auto;
    overscroll-behavior: contain;
    text-align: center;
    touch-action: pan-x;
  }

  header.landscape {
    flex-direction: row;
    padding-top: 36px;
  }

  .album {
    flex: none;
    display: grid;
    place-items: center;
    max-width: 100%;
  }

  .controls {
    box-sizing: border-box;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    max-width: 100%;
    padding: 0 12px;
  }

  header.landscape .controls {
    margin-inline: auto;
  }

  /* Finamp's Spacers: equal in portrait, 4 / 4 / 10 / 4 in landscape; a small minimum keeps the groups apart. */
  .sp {
    display: block;
    flex: 1 1 0;
    min-height: 12px;
  }

  header.landscape .sp.a,
  header.landscape .sp.b,
  header.landscape .sp.d {
    flex-grow: 4;
  }

  header.landscape .sp.c {
    flex-grow: 10;
  }

  .track,
  .area,
  .side {
    flex: none;
  }

  .area {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .corner {
    position: absolute;
    top: 10px;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--text);
    cursor: pointer;
  }

  .corner.left {
    left: 8px;
  }

  /* The top bar's right: its buttons in one row. */
  .corners {
    position: absolute;
    top: 10px;
    right: 8px;
    display: flex;
    gap: 4px;
  }

  .corners .corner {
    position: static;
  }

  .corner.on {
    color: var(--art-accent);
  }

  .corner.sleep.on :global(svg) {
    fill: currentColor;
  }

  .cover-btn {
    display: block;
    width: min(78vw, 440px);
    height: min(78vw, 440px);
    padding: 0;
    background: none;
    border: 0;
    cursor: pointer;
    touch-action: none;
  }

  /* As Finamp's: radius 8, a soft shadow under it. */
  .cover {
    display: block;
    width: 100%;
    height: 100%;
    border-radius: 8px;
    object-fit: cover;
    background: var(--surface);
    box-shadow: 0 4px 24px rgb(0 0 0 / 0.3);
    user-select: none;
  }

  /* The output: a small floating pill over the title, a button. */
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-bottom: 6px;
    padding: 2px 10px;
    border: 1px solid color-mix(in srgb, var(--text) 25%, transparent);
    border-radius: 999px;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 0.72rem;
    cursor: pointer;
  }

  .title {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    font-size: 1.6rem;
    font-weight: 650;
    line-height: 1.25;
  }

  .title.idle {
    color: var(--muted);
    font-weight: 500;
  }

  .sub {
    margin-top: 6px;
    overflow: hidden;
    color: var(--muted);
    font-size: 1.05rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .progress {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }

  .progress .t {
    flex: none;
    min-width: 3.2em;
  }

  .progress .t:last-child {
    text-align: right;
  }

  /* Finamp's slider: 24 px to hold, the track in the middle, a round handle when it can seek. */
  .seek {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    height: 24px;
  }

  .seek.can {
    cursor: pointer;
    touch-action: none;
  }

  .seek:focus-visible {
    outline: 2px solid var(--art-accent);
    outline-offset: 2px;
    border-radius: 4px;
  }

  .thumb {
    position: absolute;
    top: 50%;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    border-radius: 50%;
    background: var(--art-accent);
    pointer-events: none;
  }

  .seek.held .thumb {
    transform: scale(1.3);
  }

  .bar {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--text) 18%, transparent);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--art-accent);
    transform-origin: left;
  }

  .progress.paused .fill {
    background: var(--muted);
  }

  .progress.estimated .fill {
    opacity: 0.5;
  }

  /* Finamp's buttons row: as wide as the controls, the buttons spread evenly. */
  .transport {
    display: flex;
    align-items: center;
    justify-content: space-evenly;
  }

  .transport button {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--text);
    cursor: pointer;
  }

  /* Shuffle and repeat show their state by colour, as Finamp's. */
  .transport button.shuffle {
    color: var(--muted);
  }

  .transport button.shuffle.on {
    color: var(--art-accent, var(--accent));
  }

  /* Play/pause in Finamp's shape: a rounded square (a quarter of its size as radius), tinted with the text colour,
     its icon lifted by a soft shadow. Previous and next are plain icons. */
  .transport button.main {
    width: 62px;
    height: 62px;
    border-radius: 16px;
    background: color-mix(in srgb, var(--text) 15%, transparent);
  }

  .transport button.main :global(svg) {
    filter: drop-shadow(0 2px 1px color-mix(in srgb, var(--text) 25%, transparent));
  }

  /* The menu, the queue, the favourite and the count. */
  .side {
    display: flex;
    align-items: center;
    justify-content: space-evenly;
  }

  .act {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--text);
    cursor: pointer;
  }

  /* Info chips: small read-only pills in one row, scrolling sideways when they do not fit. */
  .chips {
    display: flex;
    justify-content: center;
    gap: 6px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .chips span {
    flex: none;
    padding: 2px 9px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: var(--muted);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .fav {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--muted);
    cursor: pointer;
  }

  .fav.on {
    color: var(--art-accent);
  }

  .fav.on :global(svg) {
    fill: currentColor;
  }

  /* The radio, lit while on (as the heart). */
  .act.radio.on {
    color: var(--art-accent);
  }

  /* A small label under a button's icon (the place in the queue), outside the button's box: the row's buttons stay
     alike, so their icons line up whatever the labels say. */
  .act.labelled {
    position: relative;
  }

  .act.labelled small {
    position: absolute;
    top: calc(100% - 4px);
    left: 50%;
    transform: translateX(-50%);
    color: var(--muted);
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
  }

  .fav:disabled {
    opacity: 0.5;
  }

  /* What gives way, in this order. */
  header.drop-gapsBig .sp {
    min-height: 6px;
  }

  header.drop-gapsBig .area {
    gap: 10px;
  }

  header.drop-buttons .transport button.main {
    width: 48px;
    height: 48px;
    border-radius: 12px;
  }

  header.drop-gapsSmall .sp {
    min-height: 0;
  }

  header.drop-gapsSmall .area {
    gap: 4px;
  }

  header.drop-chips .chips {
    display: none;
  }

  header.drop-title .title {
    display: block;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  header.drop-progress .progress {
    display: none;
  }
</style>
