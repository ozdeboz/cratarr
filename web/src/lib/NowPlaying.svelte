<script lang="ts">
  // The now-playing header, part of the app shell: shown above every page. It reads the shared state (see
  // live.svelte.ts) and shows the track, its cover, the progress bar, and how many playlists hold it.
  import { untrack } from 'svelte';
  import { ApiError, coverUrl, incomingCoverUrl, setFavorite, transport, type NowPlaying, type Transport, type Upcoming } from './api';
  import Icon from './Icon.svelte';
  import { expect, expectFailed, live, player, refresh } from './live.svelte';
  import { fit, STEPS, type Fit } from './playerLayout';
  import { prefs } from './prefs.svelte';
  import { toast } from './toast.svelte';

  /** `bar`: the header above the page, which also opens full screen over it; `side`: the full-screen player beside
   *  the page (the shell's sidebar, decision 0018). */
  let { place = 'bar' }: { place?: 'bar' | 'side' } = $props();
  const full = $derived(place === 'side' || player.full);

  const np = $derived(live.view?.now_playing ?? null);
  const memberCount = $derived((live.view?.playlists ?? []).filter((p) => p.member).length);

  // Cover: the library's cover file for a library track; otherwise (or when there is none) the album's image from the
  // server, which also covers music outside the library, such as Incoming. Candidates that fail are skipped.
  let failed = $state<Record<string, boolean>>({});
  const candidates = $derived.by(() => {
    if (!np) return [];
    const library = coverUrl(np);
    const album = np.album_id ? incomingCoverUrl(np.source, np.album_id) : null;
    return (np.addable ? [library, album] : [album, library]).filter((u): u is string => !!u);
  });
  const src = $derived(candidates.find((u) => !failed[u]) ?? null);

  // Player controls. The header shows the result at once (live.svelte.ts: `expect`) and the server confirms it; only
  // a failure says something, in a toast.
  const paused = $derived(np?.progress?.paused ?? false);
  /** Within this many seconds of the start, Previous goes to the track before; later it restarts the track. */
  const RESTART_SECS = 3;

  /** What the header shows when `next` starts playing. */
  function starting(next: { id: string; name: string; artist: string | null; album: string | null; album_id: string | null; addable: boolean }, duration: number | null, upcoming: Upcoming[]): NowPlaying {
    const at = np!;
    return {
      ...at,
      ...next,
      progress: { position_secs: 0, duration_secs: duration, paused: at.progress?.paused ?? false, estimated: false },
      favorite: at.favorite === null ? null : false,
      upcoming,
    };
  }

  function show(command: Transport) {
    if (!np) return;
    const was = np;
    if (command === 'next' && was.upcoming.length) {
      const [next, ...rest] = was.upcoming;
      expect(starting(next, next.duration_secs, rest), (s) => (!s ? 'wait' : s.id === next.id ? 'yes' : s.id === was.id ? 'wait' : 'no'));
    } else if (command === 'previous' && position > RESTART_SECS) {
      const at = position;
      expect({ ...was, progress: was.progress && { ...was.progress, position_secs: 0 } }, (s) =>
        !s ? 'wait' : s.id !== was.id ? 'no' : (s.progress?.position_secs ?? 0) < Math.max(RESTART_SECS, at - 2) ? 'yes' : 'wait',
      );
    } else if (command === 'previous' && live.before && live.before.id !== was.id) {
      const back = live.before;
      const here: Upcoming = { ...was, duration_secs: was.progress?.duration_secs ?? null };
      expect(
        starting(back, back.progress?.duration_secs ?? null, [here, ...was.upcoming]),
        (s) => (!s ? 'wait' : s.id === back.id ? 'yes' : s.id === was.id ? 'wait' : 'no'),
        // The player may restart the track instead: then it stays, without an error.
        { ms: 4000, quiet: true },
      );
    } else if (command === 'play-pause' && was.progress) {
      const want = !was.progress.paused;
      expect({ ...was, progress: { ...was.progress, paused: want, position_secs: position } }, (s) =>
        !s ? 'no' : s.id !== was.id ? 'no' : s.progress?.paused === want ? 'yes' : 'wait',
      );
    }
  }

  async function send(command: Transport) {
    if (!np) return;
    const source = np.source;
    show(command);
    try {
      await transport(command, source);
      setTimeout(refresh, 700); // the player reports back within a moment
    } catch (e) {
      expectFailed(e instanceof ApiError && !e.generic ? e.message : "The player didn't respond");
    }
  }

  // The bar runs on a local clock between polls. It is re-anchored to the server only when the server disagrees
  // with it (track, pause state, or more than 1.5 s off: a seek or drift), so it moves smoothly.
  let anchor = $state<{ id: string; pos: number; at: number; paused: boolean } | null>(null);
  let tick = $state(performance.now());

  $effect(() => {
    const p = np?.progress;
    if (!np || !p) {
      anchor = null;
      return;
    }
    const now = performance.now();
    const a = untrack(() => anchor);
    const predicted = a ? a.pos + (a.paused ? 0 : (now - a.at) / 1000) : 0;
    if (!a || a.id !== np.id || a.paused !== p.paused || Math.abs(predicted - p.position_secs) > 1.5) {
      anchor = { id: np.id, pos: p.position_secs, at: now, paused: p.paused };
    }
  });

  $effect(() => {
    if (!anchor || anchor.paused) return;
    const t = setInterval(() => {
      if (!document.hidden) tick = performance.now();
    }, 250);
    return () => clearInterval(t);
  });

  const duration = $derived(np?.progress?.duration_secs ?? null);
  const position = $derived.by(() => {
    if (!anchor) return 0;
    const p = anchor.paused ? anchor.pos : anchor.pos + Math.max(0, tick - anchor.at) / 1000;
    return duration ? Math.min(p, duration) : p;
  });

  // Favourite: shown as the server reports it; a tap shows the new state at once and asks for it.
  let favWanted = $state<{ id: string; on: boolean } | null>(null);
  const favorite = $derived(np && favWanted?.id === np.id ? favWanted.on : (np?.favorite ?? false));
  async function toggleFavorite() {
    if (!np || np.favorite === null || live.pending) return;
    const want = { id: np.id, on: !favorite };
    favWanted = want;
    try {
      live.view = await setFavorite(want.on, np.source);
    } catch (e) {
      toast.fail(e, "The favourite didn't change");
    } finally {
      // By value: `favWanted` is a state proxy, never `===` the plain object it was given.
      if (favWanted?.id === want.id && favWanted.on === want.on) favWanted = null;
    }
  }

  // Full screen: opened from the cover, the title or the expand button; closed by the collapse button, Esc, a swipe
  // down, or the navigation over the page (see App.svelte). Beside the page it has a maximise button; maximised, a
  // button takes it back beside the page.
  const open = () => (player.full = true);
  const close = () => (player.full = false);
  $effect(() => {
    if (!full) return;
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && close();
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  });

  /** Swipe up: the playlists that hold the track, on the Playlists page (full screen closes over it; beside it, it
   *  stays). */
  function toPlaylists() {
    if (place === 'bar') close();
    if (location.hash.split('?')[0] !== '#/' && location.hash !== '') location.hash = '#/';
  }

  // Gestures (decision 0018). Anywhere: swipe down closes, swipe up shows the playlists. On the cover: swipe left or
  // right skips, a tap plays or pauses, a double-tap toggles the favourite. A swipe never also counts as a tap.
  let head = $state<HTMLElement>();
  let swiped = false;
  $effect(() => {
    if (!full || !head) return;
    let from: { x: number; y: number; cover: boolean } | null = null;
    const down = (e: PointerEvent) =>
      (from = { x: e.clientX, y: e.clientY, cover: !!(e.target as Element).closest?.('.cover-btn') });
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
    if (!full) return open();
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

  // Layout (decision 0018, lib/playerLayout.ts): on every resize, and when the track changes, the player measures its
  // controls with steps dropped one by one and settles on what gives way, the cover's size and the orientation.
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
    if (!full || !head) {
      layout = null;
      return;
    }
    const ro = new ResizeObserver(() => relayout());
    ro.observe(head);
    return () => ro.disconnect();
  });
  // What is shown changes the controls' height (a long title, the pill, a progress bar or none).
  $effect(() => {
    void [np?.id, np?.name, np?.artist, np?.album, np?.player, !!np?.progress, prefs.showProgress, full];
    if (full) untrack(() => requestAnimationFrame(relayout));
  });

  function clock(secs: number) {
    const s = Math.floor(secs);
    const mm = String(Math.floor((s % 3600) / 60));
    const ss = String(s % 60).padStart(2, '0');
    return s >= 3600 ? `${Math.floor(s / 3600)}:${mm.padStart(2, '0')}:${ss}` : `${mm}:${ss}`;
  }
</script>

<header
  class:full
  class:in-sidebar={place === 'side'}
  class:landscape={full && layout?.landscape}
  class:drop-gaps={full && dropped('gaps')}
  class:drop-buttons={full && dropped('buttons')}
  class:drop-pill={full && dropped('pill')}
  class:drop-title={full && dropped('title')}
  class:drop-times={full && dropped('times')}
  class:drop-progress={full && dropped('progress')}
  style:--cover={full && layout ? `${layout.cover}px` : null}
  style:--controls={full && layout ? `${layout.controls}px` : null}
  bind:this={head}
>
  {#if full}
    {#if place === 'side'}
      <button class="collapse" aria-label="Close the player" onclick={close}><Icon name="collapse" size={26} /></button>
      <button class="maximise" aria-label="Over the page" onclick={() => (player.max = true)}><Icon name="expand" size={20} /></button>
    {:else}
      <button class="collapse" aria-label="Back to normal size" onclick={close}><Icon name="collapse" size={26} /></button>
      {#if player.roomy}
        <button class="maximise" aria-label="Beside the page" onclick={() => (player.max = false)}><Icon name="sidebar" size={22} /></button>
      {/if}
    {/if}
  {/if}
  {#if np && src}
    {#key src}
      <button class="cover-btn" aria-label={full ? 'Play or pause' : 'Full screen'} tabindex={full ? -1 : 0} onclick={coverTap}>
        <img class="cover" {src} alt="" decoding="async" draggable="false" onerror={() => (failed[src] = true)} />
      </button>
    {/key}
  {/if}
  <div class="controls" bind:this={controls}>
  <div class="np">
    {#if np}
      {#if np.player}<span class="pill">{np.player}</span>{/if}
      {#if full}
        <div class="title">{np.name}</div>
      {:else}
        <button class="title" onclick={open}>{np.name}</button>
      {/if}
      <div class="sub">{[np.artist, np.album].filter(Boolean).join(' · ')}</div>
      <div class="row">
      <div class="transport" role="group" aria-label="Player controls">
        <button aria-label="Previous track" onclick={() => send('previous')}><Icon name="previous" size={full ? 24 : 18} /></button>
        <button class="main" aria-label={paused ? 'Play' : 'Pause'} onclick={() => send('play-pause')}
          ><Icon name={paused ? 'play' : 'pause'} size={full ? 32 : 20} /></button
        >
        <button aria-label="Next track" onclick={() => send('next')}><Icon name="next" size={full ? 24 : 18} /></button>
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
          <span class="t">{np.progress.estimated ? '≈' : ''}{clock(position)}</span>
          <div
            class="bar"
            role="progressbar"
            aria-label={np.progress.paused ? 'Track position, paused' : 'Track position'}
            aria-valuemin={0}
            aria-valuemax={duration ?? undefined}
            aria-valuenow={Math.floor(position)}
            aria-valuetext={duration ? `${clock(position)} of ${clock(duration)}` : clock(position)}
          >
            <div class="fill" style:transform="scaleX({duration ? position / duration : 0})"></div>
          </div>
          <span class="t">{duration ? clock(duration) : '–:––'}</span>
        </div>
      {/if}
      </div>
    {:else}
      <div class="title idle">Nothing is playing</div>
    {/if}
  </div>
  {#if np}
    <div class="side">
    <div class="acts">
      {#if np.favorite !== null}
        <button class="fav" class:on={favorite} disabled={live.pending} aria-label={favorite ? 'Favourite' : 'Not a favourite'} aria-pressed={favorite} onclick={toggleFavorite}
          ><Icon name="heart" size={full ? 26 : 18} /></button
        >
      {/if}
      {#if !full}
        <button class="expand" aria-label="Full screen" aria-expanded="false" onclick={open}><Icon name="expand" size={16} /></button>
      {/if}
    </div>
    <div class="count" class:pending={live.pending} aria-live="polite" aria-label="In {memberCount} {memberCount === 1 ? 'playlist' : 'playlists'}">
      <span>in</span>
      <strong>{live.pending ? '–' : memberCount}</strong>
      <span>{memberCount === 1 ? 'playlist' : 'playlists'}</span>
    </div>
    </div>
  {/if}
  </div>
</header>

<style>
  /* Cover | track | favourite, expand and count. On very small screens the count and buttons go under the cover, so
     the track's column (titles, controls, progress) gets the whole width beside it. */
  header {
    position: sticky;
    top: 0;
    z-index: 2;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    grid-template-areas: 'cover np side';
    align-items: center;
    gap: 8px 16px;
    padding: 16px 0 12px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }

  /* A background layer reaching into the page's side padding (the pages' own clamp), so a row that bleeds wider
     (the playing album on Incoming) never shows through while it scrolls under the header. Not further: on
     desktop the sidebar sits right next to that padding. (A clipped box-shadow did the same but made rendering
     hang with many images below, observed in Chrome on 0.2.5.) */
  header::before {
    content: '';
    position: absolute;
    inset: 0 calc(-1 * clamp(12px, 2vw, 32px));
    z-index: -1;
    background: var(--bg);
  }


  .np {
    flex: 1;
    min-width: 0;
  }

  .progress {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 640px;
    margin-top: 8px;
    font-size: 0.8rem;
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
    background: var(--border);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    transform-origin: left;
    transition: transform 250ms linear;
  }

  .progress.paused .fill {
    background: var(--muted);
  }

  .progress.estimated .fill {
    opacity: 0.5;
  }

  .cover {
    flex: none;
    width: 56px;
    height: 56px;
    border-radius: 8px;
    object-fit: cover;
    background: var(--surface);
  }

  .title {
    font-size: 1.25rem;
    font-weight: 650;
    line-height: 1.25;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title.idle {
    color: var(--muted);
    font-weight: 500;
  }

  .sub {
    margin-top: 2px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }


  .count {
    flex: none;
    text-align: center;
    line-height: 1.1;
  }

  .count strong {
    display: block;
    font-size: 1.8rem;
    color: var(--accent);
  }

  .count span {
    display: block;
    color: var(--muted);
    font-size: 0.75rem;
  }

  .pill {
    display: inline-block;
    margin-bottom: 4px;
    padding: 1px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--muted);
    font-size: 0.72rem;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 8px;
  }

  .row .progress {
    flex: 1;
    margin-top: 0;
  }

  .transport {
    display: flex;
    flex: none;
    align-items: center;
    gap: 2px;
  }

  .transport button {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
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
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--text) 15%, transparent);
    color: var(--text);
  }

  .transport button.main :global(svg) {
    filter: drop-shadow(0 2px 1px color-mix(in srgb, var(--text) 25%, transparent));
  }

  .fav:disabled,
  .count.pending {
    opacity: 0.5;
  }

  /* The cover and the title open full screen; they look as before. */
  .cover-btn,
  button.title {
    display: block;
    padding: 0;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: inherit;
    cursor: pointer;
  }

  button.title {
    max-width: 100%;
    font-size: 1.25rem;
    font-weight: 650;
  }

  /* The expand button sits above the playlist count: no width taken from the progress bar. */
  .cover-btn {
    grid-area: cover;
  }

  .np {
    grid-area: np;
  }

  .side {
    grid-area: side;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }

  .acts {
    display: flex;
    gap: 0;
  }

  .acts button {
    width: 28px !important;
    height: 28px !important;
  }

  .fav {
    display: grid;
    place-items: center;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--muted);
    cursor: pointer;
  }

  .fav.on {
    color: var(--accent);
  }

  .fav.on :global(svg) {
    fill: currentColor;
  }

  @media (max-width: 480px) {
    header:not(.full) {
      grid-template-columns: auto minmax(0, 1fr);
      grid-template-areas:
        'cover np'
        'side np';
      align-items: start;
      column-gap: 12px;
    }

    header:not(.full) .side {
      align-self: start;
    }

    /* The progress bar on its own line under the controls, full width. */
    header:not(.full) .row {
      flex-wrap: wrap;
      row-gap: 4px;
    }

    header:not(.full) .row .progress {
      flex: 1 0 100%;
    }
  }

  .expand,
  .collapse,
  .maximise {
    display: grid;
    flex: none;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--muted);
    cursor: pointer;
  }

  /* The track's column and the count stay separate grid cells in the header; full screen they become one block. */
  .controls {
    display: contents;
  }

  /* Full screen (decision 0018): the cover and the controls share the panel above the navigation, or the sidebar
     beside the page. lib/playerLayout.ts decides, by measuring, the cover's size (--cover), the controls' width
     (--controls), portrait or landscape, and which elements give way (drop-*, first to last: gaps, large buttons,
     the player pill, the title's second line, the time labels, the progress bar). Spare room goes evenly around the
     cover and the controls. Only when even a minimal cover does not fit does the panel scroll. */
  header.full {
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
    padding: 56px 0 24px;
    border-bottom: 0;
    text-align: center;
    touch-action: pan-x;
  }

  header.full.landscape {
    grid-template-columns: var(--cover) var(--controls);
    grid-template-areas: 'cover controls';
    justify-content: space-evenly;
    align-content: center;
    align-items: center;
    column-gap: 24px;
  }

  header.full::before {
    inset: 0;
  }

  header.full.in-sidebar {
    padding: 56px 16px 24px;
  }

  header.full .collapse {
    position: absolute;
    top: 10px;
    left: 0;
  }

  header.full.in-sidebar .collapse {
    left: 8px;
  }

  header.full .maximise {
    position: absolute;
    top: 10px;
    right: 0;
  }

  header.full.in-sidebar .maximise {
    right: 8px;
  }

  header.full .cover-btn {
    grid-area: cover;
    width: var(--cover, min(78vw, 440px));
    height: var(--cover, min(78vw, 440px));
    cursor: pointer;
    touch-action: none;
  }

  header.full .cover {
    width: 100%;
    height: 100%;
    border-radius: 14px;
    user-select: none;
  }

  header.full .controls {
    grid-area: controls;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 20px;
    width: min(100%, var(--controls, 560px));
  }

  header.full .np {
    flex: none;
    width: 100%;
  }

  header.full .title {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    font-size: 1.6rem;
    white-space: normal;
  }

  header.full .sub {
    margin-top: 6px;
    font-size: 1.05rem;
  }

  header.full .row {
    flex-direction: column-reverse;
    gap: 18px;
    margin-top: 22px;
  }

  header.full .row .progress {
    width: 100%;
    max-width: none;
    font-size: 0.85rem;
  }

  header.full .transport {
    gap: 28px;
  }

  header.full .transport button {
    width: 48px;
    height: 48px;
  }

  header.full .transport button.main {
    width: 62px;
    height: 62px;
    border-radius: 16px;
  }

  header.full .acts button {
    width: 44px !important;
    height: 44px !important;
  }

  /* The favourite next to the count, not above it: less height. */
  header.full .side {
    flex-direction: row;
    gap: 20px;
  }

  /* What gives way, in this order. */
  header.full.drop-gaps .controls {
    gap: 8px;
  }

  header.full.drop-gaps .row {
    gap: 8px;
    margin-top: 10px;
  }

  header.full.drop-buttons .transport {
    gap: 14px;
  }

  header.full.drop-buttons .transport button.main {
    width: 48px;
    height: 48px;
    border-radius: 12px;
  }

  header.full.drop-pill .pill {
    display: none;
  }

  header.full.drop-title .title {
    display: block;
    white-space: nowrap;
  }

  header.full.drop-times .progress .t {
    display: none;
  }

  header.full.drop-progress .progress {
    display: none;
  }
</style>
