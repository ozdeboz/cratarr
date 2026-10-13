<script lang="ts">
  // The full player (decision 0018): over the page (`overlay`, opened from the mini-player) or beside it (`side`, the
  // shell's sidebar on wide windows). It shows the cover, the track, the controls, the progress, the favourite and
  // how many playlists hold the track, laid out by lib/playerLayout.ts, on the blurred cover in its colours.
  import { untrack } from 'svelte';
  import { incomingCoverUrl, jumpTo } from './api';
  import Icon from './Icon.svelte';
  import { live, player, refresh } from './live.svelte';
  import TrackMenu from './TrackMenu.svelte';
  import { toast } from './toast.svelte';
  import { fit, STEPS, type Fit } from './playerLayout';
  import { prefs } from './prefs.svelte';
  import { art, clockText, coverFailed, coverSrc, duration, favorite, paused, position, send, toggleFavorite } from './remote.svelte';

  let { place }: { place: 'overlay' | 'side' } = $props();

  const np = $derived(live.view?.now_playing ?? null);
  const memberCount = $derived((live.view?.playlists ?? []).filter((p) => p.member).length);
  const src = $derived(coverSrc());
  const pos = $derived(position());
  const dur = $derived(duration());

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
  let queueOpen = $state(false);
  const openMenu = (e: Event) => (menuAt = (e.currentTarget as HTMLElement).getBoundingClientRect());
  async function jump(id: string) {
    try {
      await jumpTo(id, np?.source);
      setTimeout(refresh, 700);
    } catch (e) {
      toast.fail(e, "The player didn't go to that track");
    }
  }

  // Closing: over the page the chevron, Esc and a swipe down close it; beside the page they fold it to the
  // mini-player until that is tapped (not remembered). Maximise puts it over the page, the sidebar button back beside
  // it; both are remembered (prefs.sidebar).
  function close() {
    if (place === 'side') player.min = true;
    else player.full = false;
  }
  function maximise() {
    prefs.sidebar = false;
    player.full = true;
  }
  function toSidebar() {
    prefs.sidebar = true;
    player.min = false;
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

  /** Swipe up: the playlists that hold the track, on the Playlists page (over the page it closes first). */
  function toPlaylists() {
    if (place === 'overlay') player.full = false;
    if (location.hash.split('?')[0] !== '#/' && location.hash !== '') location.hash = '#/';
  }

  // Gestures. Anywhere: swipe down closes, swipe up shows the playlists. On the cover: swipe left or right skips, a
  // tap plays or pauses, a double-tap toggles the favourite. A swipe never also counts as a tap.
  let head = $state<HTMLElement>();
  let swiped = false;
  $effect(() => {
    if (!head) return;
    let from: { x: number; y: number; cover: boolean } | null = null;
    const down = (e: PointerEvent) => (from = { x: e.clientX, y: e.clientY, cover: !!(e.target as Element).closest?.('.cover-btn') });
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
        toPlaylists();
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
    // Measuring sets the classes directly and puts them back afterwards; the state then sets the outcome.
    const before = { cls: el.className, controls: el.style.getPropertyValue('--controls') };
    const measure = (landscape: boolean, level: number, controlsWidth: number) => {
      el.classList.toggle('landscape', landscape);
      STEPS.forEach((step, i) => el.classList.toggle(`drop-${step}`, level > i));
      el.style.setProperty('--controls', `${controlsWidth}px`);
      return box.getBoundingClientRect().height;
    };
    const next = fit(width, height, measure);
    el.className = before.cls;
    el.style.setProperty('--controls', before.controls);
    if (!layout || next.landscape !== layout.landscape || next.level !== layout.level || next.cover !== layout.cover || next.controls !== layout.controls) {
      layout = next;
    }
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
  {#if src}
    {#key src}<img class="backdrop" {src} alt="" aria-hidden="true" />{/key}
  {/if}
  <div class="veil"></div>
  <header
    class:landscape={layout?.landscape}
    class:drop-gaps={dropped('gaps')}
    class:drop-buttons={dropped('buttons')}
    class:drop-pill={dropped('pill')}
    class:drop-chips={dropped('chips')}
    class:drop-title={dropped('title')}
    class:drop-times={dropped('times')}
    class:drop-progress={dropped('progress')}
    style:--cover={layout ? `${layout.cover}px` : null}
    style:--controls={layout ? `${layout.controls}px` : null}
    bind:this={head}
  >
    <button class="corner left" aria-label={place === 'side' ? 'Fold the player' : 'Close the player'} onclick={close}><Icon name="collapse" size={26} /></button>
    {#if place === 'side'}
      <button class="corner right" aria-label="Over the page" onclick={maximise}><Icon name="expand" size={20} /></button>
    {:else if player.roomy}
      <button class="corner right" aria-label="Beside the page" onclick={toSidebar}><Icon name="sidebar" size={22} /></button>
    {/if}
    {#if np && src}
      {#key src}
        <button class="cover-btn" aria-label="Play or pause" tabindex="-1" onclick={coverTap} oncontextmenu={(e) => (e.preventDefault(), openMenu(e))}>
          <img class="cover" {src} alt="" decoding="async" draggable="false" onerror={() => coverFailed(src)} />
        </button>
      {/key}
    {/if}
    <div class="controls" bind:this={controls}>
      <div class="np">
        {#if np}
          {#if np.player}<span class="pill">{np.player}</span>{/if}
          <div class="title">{np.name}</div>
          <div class="sub">{[np.artist, np.album].filter(Boolean).join(' · ')}</div>
          {#if chips.length}
            <div class="chips" aria-label="About this track">
              {#each chips as c (c)}<span>{c}</span>{/each}
            </div>
          {/if}
          <div class="row">
            <div class="transport" role="group" aria-label="Player controls">
              <button aria-label="Previous track" onclick={() => send('previous')}><Icon name="previous" size={24} /></button>
              <button class="main" aria-label={paused() ? 'Play' : 'Pause'} onclick={() => send('play-pause')}
                ><Icon name={paused() ? 'play' : 'pause'} size={32} /></button
              >
              <button aria-label="Next track" onclick={() => send('next')}><Icon name="next" size={24} /></button>
            </div>
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
                <div
                  class="bar"
                  role="progressbar"
                  aria-label={np.progress.paused ? 'Track position, paused' : 'Track position'}
                  aria-valuemin={0}
                  aria-valuemax={dur ?? undefined}
                  aria-valuenow={Math.floor(pos)}
                  aria-valuetext={dur ? `${clockText(pos)} of ${clockText(dur)}` : clockText(pos)}
                >
                  <div class="fill" style:transform="scaleX({dur ? pos / dur : 0})"></div>
                </div>
                <span class="t">{dur ? clockText(dur) : '–:––'}</span>
              </div>
            {/if}
          </div>
        {:else}
          <div class="title idle">Nothing is playing</div>
        {/if}
      </div>
      {#if np}
        <div class="side">
          <button class="act" aria-label="Track menu" aria-haspopup="menu" onclick={openMenu}><Icon name="menu" size={24} /></button>
          <button class="act" aria-label="Queue" aria-expanded={queueOpen} onclick={() => (queueOpen = true)}><Icon name="playlists" size={24} /></button>
          {#if np.favorite !== null}
            <button class="fav" class:on={favorite()} disabled={live.pending} aria-label={favorite() ? 'Favourite' : 'Not a favourite'} aria-pressed={favorite()} onclick={toggleFavorite}
              ><Icon name="heart" size={26} /></button
            >
          {/if}
          <div class="count" class:pending={live.pending} aria-live="polite" aria-label="In {memberCount} {memberCount === 1 ? 'playlist' : 'playlists'}">
            <span>in</span>
            <strong>{live.pending ? '–' : memberCount}</strong>
            <span>{memberCount === 1 ? 'playlist' : 'playlists'}</span>
          </div>
        </div>
      {/if}
    </div>
  </header>

  {#if queueOpen && np}
    <!-- The queue (decision 0019): the playing track, then what comes next as the player reports it; a tap plays it. -->
    <div class="queue" role="dialog" aria-label="Queue">
      <div class="qhead">
        <button class="corner left" aria-label="Close the queue" onclick={() => (queueOpen = false)}><Icon name="collapse" size={26} /></button>
        <span>Queue</span>
      </div>
      <div class="qnow">
        {#if src}<img {src} alt="" />{/if}
        <div><strong>{np.name}</strong><span>{np.artist ?? ''}</span></div>
      </div>
      {#if np.upcoming.length}
        <ol>
          {#each np.upcoming as t, i (t.id + ':' + i)}
            <li>
              <button onclick={() => jump(t.id)}>
                {#if t.album_id}<img src={incomingCoverUrl(np.source, t.album_id)} alt="" loading="lazy" />{:else}<span class="noimg"></span>{/if}
                <div><strong>{t.name}</strong><span>{t.artist ?? ''}</span></div>
                {#if t.duration_secs}<span class="d">{clockText(t.duration_secs)}</span>{/if}
              </button>
            </li>
          {/each}
        </ol>
      {:else}
        <p class="empty">Nothing comes next.</p>
      {/if}
    </div>
  {/if}
</div>

{#if menuAt}
  <TrackMenu anchor={menuAt} onclose={() => (menuAt = null)} />
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
    transition: --art-accent 1s;
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

  /* lib/playerLayout.ts decides, by measuring, the cover's size (--cover), the controls' width (--controls), portrait
     or landscape, and which elements give way (drop-*, first to last: gaps, large buttons, the player pill, the
     title's second line, the time labels, the progress bar). Spare room goes evenly around the cover and the
     controls. Only when even a minimal cover does not fit does the player scroll. */
  header {
    box-sizing: border-box;
    position: relative;
    display: grid;
    flex: 1 1 0;
    min-height: 0;
    grid-template-columns: minmax(0, 1fr);
    grid-template-areas: 'cover' 'controls';
    justify-items: center;
    align-content: space-evenly;
    row-gap: 24px;
    padding: 56px clamp(12px, 3vw, 24px) calc(24px + env(safe-area-inset-bottom));
    overflow-y: auto;
    text-align: center;
    touch-action: pan-x;
  }

  .in-sidebar header {
    padding: 56px 16px 24px;
  }

  header.landscape {
    grid-template-columns: var(--cover) var(--controls);
    grid-template-areas: 'cover controls';
    justify-content: space-evenly;
    align-content: center;
    align-items: center;
    column-gap: 24px;
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

  .corner.right {
    right: 8px;
  }

  .cover-btn {
    grid-area: cover;
    display: block;
    width: var(--cover, min(78vw, 440px));
    height: var(--cover, min(78vw, 440px));
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

  .controls {
    grid-area: controls;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 20px;
    width: min(100%, var(--controls, 560px));
  }

  .np {
    width: 100%;
    min-width: 0;
  }

  .pill {
    display: inline-block;
    margin-bottom: 6px;
    padding: 1px 8px;
    border: 1px solid color-mix(in srgb, var(--text) 25%, transparent);
    border-radius: 999px;
    color: var(--muted);
    font-size: 0.72rem;
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

  .row {
    display: flex;
    flex-direction: column-reverse;
    align-items: center;
    gap: 18px;
    margin-top: 22px;
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
    transition: transform 250ms linear;
  }

  .progress.paused .fill {
    background: var(--muted);
  }

  .progress.estimated .fill {
    opacity: 0.5;
  }

  .transport {
    display: flex;
    align-items: center;
    gap: 28px;
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
    gap: 14px;
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
    margin-top: 10px;
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

  /* The queue: a sheet over the player (Finamp's opens at 92 %), the playing track pinned at its top. */
  .queue {
    position: absolute;
    inset: 8% 0 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    border-radius: 16px 16px 0 0;
    background: var(--surface);
    box-shadow: 0 -8px 30px rgb(0 0 0 / 0.4);
  }

  .qhead {
    position: relative;
    padding: 18px 16px 10px;
    font-weight: 650;
    text-align: center;
  }

  .qhead .corner {
    top: 6px;
  }

  .qnow,
  .queue li button {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 8px 16px;
    text-align: left;
  }

  .qnow {
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--art-accent) 14%, transparent);
  }

  .qnow img,
  .queue img,
  .noimg {
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 6px;
    object-fit: cover;
    background: var(--bg);
  }

  .qnow div,
  .queue li div {
    flex: 1;
    min-width: 0;
  }

  .qnow strong,
  .qnow span,
  .queue li strong,
  .queue li div span {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qnow span,
  .queue li div span,
  .queue .d {
    color: var(--muted);
    font-size: 0.85rem;
  }

  .queue ol {
    flex: 1;
    margin: 0;
    padding: 4px 0 calc(12px + env(safe-area-inset-bottom));
    overflow-y: auto;
    list-style: none;
  }

  .queue li button {
    min-height: 56px;
    background: none;
    border: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .queue li button:hover,
  .queue li button:focus-visible {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  .queue .d {
    flex: none;
    font-variant-numeric: tabular-nums;
  }

  .empty {
    margin: 16px;
    color: var(--muted);
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

  .count {
    text-align: center;
    line-height: 1.1;
  }

  .count strong {
    display: block;
    font-size: 1.8rem;
    color: var(--art-accent);
  }

  .count span {
    display: block;
    color: var(--muted);
    font-size: 0.75rem;
  }

  .fav:disabled,
  .count.pending {
    opacity: 0.5;
  }

  /* What gives way, in this order. */
  header.drop-gaps .controls {
    gap: 8px;
  }

  header.drop-gaps .row {
    gap: 8px;
    margin-top: 10px;
  }

  header.drop-buttons .transport {
    gap: 14px;
  }

  header.drop-buttons .transport button.main {
    width: 48px;
    height: 48px;
    border-radius: 12px;
  }

  header.drop-pill .pill {
    display: none;
  }

  header.drop-chips .chips {
    display: none;
  }

  header.drop-title .title {
    display: block;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  header.drop-times .progress .t {
    display: none;
  }

  header.drop-progress .progress {
    display: none;
  }
</style>
