<script lang="ts">
  // The full player (decision 0018): over the page (`overlay`, opened from the mini-player) or beside it (`side`, the
  // shell's sidebar on wide windows). It shows the cover, the track, the controls, the progress, the favourite and
  // how many playlists hold the track, laid out by lib/playerLayout.ts, on the blurred cover in its colours.
  import { untrack } from 'svelte';
  import { linear } from 'svelte/easing';
  import { fade } from 'svelte/transition';
  import { reducedMotion } from './motion';
  import Icon from './Icon.svelte';
  import { live, player, refresh } from './live.svelte';
  import TrackMenu from './TrackMenu.svelte';
  import QueueSheet from './QueueSheet.svelte';
  import OutputSheet from './OutputSheet.svelte';
  import SleepSheet from './SleepSheet.svelte';
  import BandSheet from './BandSheet.svelte';
  import { sleep, sleepLeft } from './sleep.svelte';
  import { fit, STEPS, type Fit } from './playerLayout';
  import { prefs, SEEK_STEP, type CoverTouch, type SwipeDown, type SwipeSide, type SwipeUp } from './prefs.svelte';
  import { art, clockText, coverFailed, coverSrc, shownCover, duration, favorite, paused, pending, playing, position, seekTo, send, toggleFavorite } from './remote.svelte';
  import Waveform from './Waveform.svelte';
  import WaveLens from './WaveLens.svelte';
  import { heardOf, loadWaveform, waveformOf } from './waveforms.svelte';
  import { SILENT, loadSpectrum, spectrumOf } from './bandWaves.svelte';
  import { tuneOf } from './bandTune.svelte';
  import BandGlass from './BandGlass.svelte';
  import { isLocal, playsHere, shownPlaylists } from './output.svelte';
  import { local, restoreUpcoming, setRepeat, shuffleUpcoming } from './localPlayer.svelte';
  import QueueMenu from './QueueMenu.svelte';
  import { playingCrateOuts } from './crates.svelte';
  import { localMode } from './deviceMusic.svelte';
  import HeartSheet from './HeartSheet.svelte';
  import { isCircle } from './circle';
  import Cover from './Cover.svelte';
  import CoverOverview from './CoverOverview.svelte';
  import { detailsOf, factsOf, withHeard } from './trackFacts.svelte';
  import { BANDCAMP } from './bandcampPreview';
  import { longpress } from './longpress';
  import MoreMusicSheet from './MoreMusicSheet.svelte';
  import { visuals } from './visuals/shown.svelte';
  import { albumSubject, keepViewed, panel, SETTINGS, viewOver } from './panel.svelte';
  import { goToArtist, mainArtist } from './albums.svelte';
  import { artistFavorite } from './api';
  import { toast } from './toast.svelte';

  let { place }: { place: 'overlay' | 'side' } = $props();

  const np = $derived(playing());

  // The artist as a chip (the owner, 2026-10-17): it opens their panel, the player closing (lib/albums goToArtist); a
  // "feat." credit goes to the main artist, as the track menu's Go to artist. A favourite artist's chip has a heart.
  const by = $derived(mainArtist(np?.artist));
  // Apart from `np`, which is new at each report: asked once per artist, not at every tick.
  const bySource = $derived(np?.source ?? null);
  let byFavorite = $state<boolean | null>(null);
  $effect(() => {
    const source = bySource;
    const name = by;
    byFavorite = null;
    if (!source || !name) return;
    let live = true;
    artistFavorite(source, name).then(
      (r) => live && (byFavorite = r.favorite),
      () => {},
    );
    return () => {
      live = false;
    };
  });
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
  // The track's waveform (decision 0035), where it has one: the seek bar draws it instead of its line.
  $effect(() => {
    if (np) loadWaveform(np.source, np.id);
  });
  const wave = $derived(np ? waveformOf(np.source, np.id) : null);
  // The bands (lib/BandGlass), when chosen: the track's spectrum, read only then.
  $effect(() => {
    if (np && dur && prefs.playerBands !== 'off') loadSpectrum(np.source, np.id, dur);
  });
  const spectrum = $derived(np && prefs.playerBands !== 'off' ? spectrumOf(np.source, np.id) : null);
  /** The chosen look's settings (lib/bandTune: its own defaults, the user's changes over them). */
  const tune = $derived(tuneOf(prefs.playerBands === 'off' ? 'sirius' : prefs.playerBands));
  /** Where the handle is held (seconds), while the finger is down; only then. */
  let held = $state<number | null>(null);
  /** Where it was let go (or a key sent it), until the player is there: the bar shows it, then hands over to where it
   *  plays in one step, on arrival (the owner, 2026-10-25: the two "structurally unable to differ", the takeover
   *  atomic). Not a fixed time: a player slow to say where it is now made the bar jump back, then forward. */
  let sought = $state<number | null>(null);
  let soughtTimer: ReturnType<typeof setTimeout> | undefined;
  // There: within half a second of it (a player reports in steps). Never there (refused, or the track changed): back
  // to where it plays after 3 s.
  $effect(() => {
    if (sought !== null && Math.abs(position() - sought) <= 0.5) sought = null;
  });
  /** A waveform that shows something (a Bandcamp analysis whose parts did not decode gives bins of nothing: a flat
   *  line, the plain bar's). It decides the thumb or the lens and the white line, everywhere; while held, as it was
   *  when the finger went down (a waveform coming in then takes over on release, not under the finger). */
  const shapedNow = $derived(!!wave && wave.some((v) => v > 0));
  let shapedHeld = $state<boolean | null>(null);
  const shaped = $derived(shapedHeld ?? shapedNow);
  // While held, where it plays stays marked, still moving (the owner, 2026-10-24: "something to aim for"); held within
  // SNAP_PX of it, the handle snaps onto it, and let go there nothing is sought (cancelling the seek). Only while the
  // finger is near the bar (SNAP_AWAY_PX above or below it): dragged further off, it comes loose (the owner,
  // 2026-10-24), so a seek just beside where it plays can still be made.
  const SNAP_PX = 12;
  const SNAP_AWAY_PX = 40;
  let snapped = $state(false);
  /** The seek bar's width (px) while held. */
  let seekWidth = 0;
  const pos = $derived(snapped ? position() : (held ?? sought ?? position()));
  /** Where it plays, 0..1, while the handle is held elsewhere over a waveform (a white line there); the plain bar has
   *  none (the owner, 2026-10-25: no white lines without a waveform). */
  const nowMark = $derived(held !== null && !snapped && shaped && dur ? position() / dur : null);
  /** Held at `secs`, `away` px above or below the bar: on the playing position (snapped) or not. */
  function hold(secs: number, away = 0) {
    const near = !!dur && seekWidth > 0 && away <= SNAP_AWAY_PX && (Math.abs(secs - position()) / dur) * seekWidth <= SNAP_PX;
    if (near && !snapped) navigator.vibrate?.(8);
    snapped = near;
    held = secs;
  }
  const at = (e: PointerEvent, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * (dur ?? 0);
  };
  /** How far (px) the pointer is above or below the bar; 0 on it. */
  const awayFrom = (e: PointerEvent, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    return Math.max(0, r.top - e.clientY, e.clientY - r.bottom);
  };
  // The waveform lens (the owner, 2026-10-19; lib/WaveLens): while the bar is held over a waveform, where it is, and
  // the room it has (the player's edges).
  let lens = $state<{ width: number; middle: number; waveHeight: number; min: number; max: number } | null>(null);
  function seekDown(e: PointerEvent) {
    if (!canSeek) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    shapedHeld = shapedNow;
    seekWidth = el.getBoundingClientRect().width;
    hold(at(e, el));
    const r = el.getBoundingClientRect();
    const room = head?.getBoundingClientRect() ?? r;
    const w = el.querySelector('.wave')?.getBoundingClientRect();
    lens = shapedHeld && w ? { width: r.width, middle: r.height / 2, waveHeight: w.height, min: room.left - r.left + 75, max: room.right - r.left - 75 } : null;
  }
  function seekMove(e: PointerEvent) {
    if (held === null || !(e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId)) return;
    const el = e.currentTarget as HTMLElement;
    hold(at(e, el), awayFrom(e, el));
  }
  function seekUp(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    lens = null;
    if (held === null || !el.hasPointerCapture(e.pointerId)) return;
    // Let go on the playing position: nothing changes.
    if (snapped) return letGo();
    release(at(e, el));
  }
  function letGo() {
    held = null;
    snapped = false;
    shapedHeld = null;
    lens = null;
  }
  /** Seek, the bar on it until the player is there (`sought`). */
  function release(secs: number) {
    letGo();
    sought = secs;
    seekTo(secs);
    clearTimeout(soughtTimer);
    soughtTimer = setTimeout(() => (sought = null), 3000);
  }
  $effect(() => () => clearTimeout(soughtTimer));
  function seekKey(e: KeyboardEvent) {
    if (!canSeek || !dur) return;
    const now = position();
    const to =
      e.key === 'ArrowLeft' ? now - 5 : e.key === 'ArrowRight' ? now + 5 : e.key === 'Home' ? 0 : e.key === 'End' ? dur - 1 : null;
    if (to === null) return;
    e.preventDefault();
    release(Math.min(dur, Math.max(0, to)));
  }

  // The track menu (the menu button, or a right-click on the cover) and the queue (decision 0019).
  let menuAt = $state<DOMRect | null>(null);
  // The menu's entries, or straight to its playlists (the top bar's playlists button).
  let menuStart = $state<'menu' | 'lists'>('menu');
  // A long press on the heart (the owner): Favourite, the crate's Excludes (decision 0024, amended 2026-10-19: the
  // track or its album, from the crate the queue came from, its own tracks; here only, not the track's menu: "Heart
  // sheet only"), and Exclude globally (lib/HeartSheet).
  let heartMenu = $state(false);

  // More music's sheet (Radio and Autoplay), from the radio button.
  let moreOpen = $state(false);
  let queueOpen = $state(false);
  /** The queue's menu (a long-press on the Queue button, lib/QueueMenu). */
  let queueMenuOpen = $state(false);
  function shuffleOnce() {
    const before = shuffleUpcoming();
    if (before) toast.show({ text: 'Shuffled.', tone: 'done', act: { label: 'Undo', run: () => restoreUpcoming(before) } });
  }
  let outputOpen = $state(false);
  let sleepOpen = $state(false);
  let bandsOpen = $state(false);
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
  // Full screen, where the browser can: over the page, then the whole screen; full screen, the same button leaves it
  // (64.2: it went away, and only the browser's Esc or back left).
  const canFullscreen = typeof document !== 'undefined' && !!document.fullscreenEnabled && typeof document.documentElement.requestFullscreen === 'function';
  let fullscreen = $state(typeof document !== 'undefined' && !!document.fullscreenElement);
  $effect(() => {
    const changed = () => (fullscreen = !!document.fullscreenElement);
    document.addEventListener('fullscreenchange', changed);
    return () => document.removeEventListener('fullscreenchange', changed);
  });
  function goFullscreen() {
    document.documentElement.requestFullscreen({ navigationUI: 'hide' }).catch(() => {});
    if (place === 'side') maximise();
  }
  const leaveFullscreen = () => document.exitFullscreen().catch(() => {});
  // Beside the page again: the player moves into the panel, over what it shows there (a playlist, a crate, a part of
  // Settings, Health or Insights), as the mini-player's tap brings it; folding it shows that again (the owner,
  // 2026-10-24: on Settings it went to the mini-player instead).
  function toSidebar() {
    prefs.sidebar = true;
    player.min = false;
    panel.playerTop = true;
    player.full = false;
  }
  $effect(() => {
    if (place !== 'overlay') return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || menuAt || visuals.open) return;
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

  // Gestures, over the page and beside it. Anywhere: a swipe up or down does what Settings › Player says for that half
  // of the player (the owner, 2026-10-26: four of them; by default up the queue, down closing). On the cover: swipe left or right skips, a
  // tap plays or pauses, a double-tap toggles the favourite, a circle shows the overview (lib/CoverOverview; the owner,
  // 2026-10-19: the cover blurred, what is known about the track over it; a tap or another circle brings the cover back;
  // not for the device's own music). A swipe or a circle never also counts as a tap.
  let head = $state<HTMLElement>();
  let swiped = false;
  let overview = $state(false);
  // The row's facts, short, in the user's order; those not known for this track are left out (lib/trackFacts). Worked
  // out again only when the track or what is known of it changes: the playing track is told again with each move of
  // its position, and the dates in it cost a frame's worth each time (2026-10-24, profiled on the tablet).
  const factsKey = $derived(np && np.source !== 'device' ? `${np.source}/${np.id}/${JSON.stringify(np.info ?? null)}` : '');
  const rowFacts = $derived.by(() => {
    const key = factsKey;
    if (!key || !prefs.playerRow || !prefs.playerFacts.length) return [];
    const now = untrack(() => np);
    if (!now) return [];
    const known = factsOf(withHeard(detailsOf(now.source, now.id), heardOf(now.source, now.id)), now.info ?? null);
    const facts = prefs.playerFacts.flatMap((id) => (known[id] ? [{ id: id as string, short: known[id].short }] : []));
    // A Bandcamp preview's row says where it plays from (the owner, 2026-10-25).
    return now.source === BANDCAMP ? [{ id: 'bandcamp', short: 'Bandcamp' }, ...facts] : facts;
  });
  /** The row's Add: Settings → Player, where the row's facts are chosen; the player over the page gives way. */
  function toRowSettings() {
    player.full = false;
    location.hash = `#/settings?view=${encodeURIComponent(SETTINGS + 'player')}`;
  }
  /** The album's name: the album in the panel, as the artist's chip opens the artist. */
  function toAlbum() {
    const t = np;
    if (!t?.album_id) return;
    player.full = false;
    viewOver(albumSubject(t.source, t.album_id));
  }
  // A new track shows its cover (its id: the player's state is a new object on every report).
  const trackId = $derived(np ? `${np.source}/${np.id}` : null);
  $effect(() => {
    void trackId;
    overview = false;
  });
  $effect(() => {
    if (!head) return;
    let from: { x: number; y: number; left: boolean; cover: boolean; path: { x: number; y: number }[] } | null = null;
    const down = (e: PointerEvent) => {
      // The seek bar's drags are its own.
      if ((e.target as Element).closest?.('.seek')) return;
      const r = head!.getBoundingClientRect();
      from = { x: e.clientX, y: e.clientY, left: e.clientX < r.left + r.width / 2, cover: !!(e.target as Element).closest?.('.cover-btn'), path: [{ x: e.clientX, y: e.clientY }] };
    };
    const move = (e: PointerEvent) => {
      if (from?.cover && e.buttons) from.path.push({ x: e.clientX, y: e.clientY });
    };
    const up = (e: PointerEvent) => {
      if (!from) return;
      const [dx, dy] = [e.clientX - from.x, e.clientY - from.y];
      const onCover = from.cover;
      const path = from.path;
      const left = from.left;
      from = null;
      if (onCover && isCircle(path)) {
        swiped = true;
        touch(prefs.playerSwipes.circle);
      } else if (onCover && Math.abs(dx) > 60 && Math.abs(dx) > 2 * Math.abs(dy)) {
        swiped = true;
        side(dx < 0 ? prefs.playerSwipes.left : prefs.playerSwipes.right);
      } else if (dy > 80 && dy > 2 * Math.abs(dx)) {
        swiped = true;
        swipe(left ? prefs.playerSwipes.downLeft : prefs.playerSwipes.downRight);
      } else if (dy < -80 && -dy > 2 * Math.abs(dx)) {
        swiped = true;
        swipe(left ? prefs.playerSwipes.upLeft : prefs.playerSwipes.upRight);
      }
    };
    const el = head;
    el.addEventListener('pointerdown', down);
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
    return () => {
      el.removeEventListener('pointerdown', down);
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
  });

  /** What a swipe left or right on the cover was chosen to do: a track on or back, or a seek. */
  function side(what: SwipeSide) {
    if (what === 'next' || what === 'previous') return send(what);
    const at = position();
    seekTo(Math.max(0, what === 'forward' ? Math.min((dur ?? at) - 1, at + SEEK_STEP) : at - SEEK_STEP));
  }
  /** What a tap, a double tap or a circle on the cover was chosen to do. */
  function touch(what: CoverTouch) {
    if (what === 'playPause') send('play-pause');
    else if (what === 'favourite') toggleFavorite();
    else if (what === 'album') toAlbum();
    else overview = !overview && np?.source !== 'device';
  }
  /** What a swipe up or down was chosen to do (Settings › Player). */
  function swipe(what: SwipeUp | SwipeDown) {
    if (what === 'close') return close();
    if (what === 'visuals') return void (visuals.open = true);
    if (!playing()) return;
    if (what === 'queue') return void (queueOpen = true);
    menuStart = what === 'playlists' ? 'lists' : 'menu';
    menuAt = head?.getBoundingClientRect() ?? null;
  }

  /** A tap waits this long for a second one (double-tap: favourite) before it plays or pauses. */
  const DOUBLE_TAP_MS = 250;
  let tapTimer: ReturnType<typeof setTimeout> | null = null;
  function coverTap() {
    if (swiped) {
      swiped = false;
      return;
    }
    // A tap on the overview brings the cover back.
    if (overview) {
      overview = false;
      return;
    }
    if (tapTimer) {
      clearTimeout(tapTimer);
      tapTimer = null;
      touch(prefs.playerSwipes.doubleTap);
      return;
    }
    tapTimer = setTimeout(() => {
      tapTimer = null;
      touch(prefs.playerSwipes.tap);
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
  // What is shown changes the controls' height (a long title, a progress bar or none). As one value: `np` is
  // new at every report, and laying out again at each would be many times a second.
  const shown = $derived(JSON.stringify([np?.id, np?.name, np?.artist, np?.album, np?.player, !!np?.progress, prefs.playerRow, rowFacts.length, prefs.playerBands !== 'off']));
  $effect(() => {
    void shown;
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
    class:drop-bands={dropped('bands')}
    class:drop-gapsBig={dropped('gapsBig')}
    class:drop-buttons={dropped('buttons')}
    class:drop-gapsSmall={dropped('gapsSmall')}
    class:drop-facts={dropped('facts')}
    class:drop-title={dropped('title')}
    class:drop-progress={dropped('progress')}
    bind:this={head}
  >
    <button class="corner left" aria-label={place === 'side' ? 'Fold the player' : 'Close the player'} onclick={close}><Icon name="collapse" size={26} /></button>
    <!-- Top right, icons only (what is on shows lit): the track's playlists (lit while it is in one), the sleep timer
         (only where the page holds the queue; lit while one runs), the full-screen visuals, and where the player goes. -->
    <div class="corners">
      {#if np}
        <button class="corner" class:on={memberCount > 0} aria-label="Add to playlist; {inPlaylists}" aria-haspopup="menu" onclick={(e) => openMenu(e, 'lists')}
          ><Icon name="playlists" size={22} /></button
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
      <!-- Not in local mode (decision 0052; the owner: "In local mode, the visuals feature is not supported"). -->
      {#if np && !localMode()}
        <button class="corner" aria-label="Visuals" aria-haspopup="dialog" onclick={() => (visuals.open = true)}><Icon name="visuals" size={22} /></button>
      {/if}
      {#if place === 'side'}
        <button class="corner" aria-label="Over the page" onclick={maximise}><Icon name="expand" size={20} /></button>
      {:else if player.roomy}
        <button class="corner" aria-label="Beside the page" onclick={toSidebar}><Icon name="sidebar" size={22} /></button>
      {/if}
      {#if canFullscreen}
        <button class="corner" aria-label={fullscreen ? 'Exit full screen' : 'Full screen'} onclick={fullscreen ? leaveFullscreen : goFullscreen}
          ><Icon name={fullscreen ? 'exitFullscreen' : 'fullscreen'} size={20} /></button
        >
      {/if}
    </div>
    <!-- Finamp's layout: the cover's region, then the controls' box with three groups spread in it (track name,
         controls, actions); in landscape side by side. Sizes from lib/playerLayout.ts. -->
    <div class="album" style:width={layout ? `${layout.album.width}px` : null} style:height={layout ? `${layout.album.height}px` : null}>
      <!-- the stand-in only when there is no cover to load (lib/Cover), not while one loads -->
      {#if np && (src || !coverSrc())}
        {#key src}
          <button
            class="cover-btn"
            class:blurred={overview}
            style:width={layout ? `${layout.cover}px` : null}
            style:height={layout ? `${layout.cover}px` : null}
            aria-label="Play or pause"
            tabindex="-1"
            onclick={coverTap}
            oncontextmenu={(e) => (e.preventDefault(), openMenu(e))}
          >
            <Cover {src} lazy={false} onfail={coverFailed} />
            {#if overview}<CoverOverview track={np} />{/if}
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
          <div class="title">{np.name}</div>
          <div class="sub">
            {#if np.artist}<button class="artist" disabled={!by} onclick={() => by && goToArtist(np.source, by)}
                ><span>{np.artist}</span>{#if byFavorite}<i class="liked" role="img" aria-label="Favourite"><Icon name="heart" size={14} /></i>{/if}</button
              >{/if}{#if np.artist && np.album}<span class="dot">·</span>{/if}{#if np.album && np.album_id}<button class="album-name" onclick={toAlbum}>{np.album}</button
              >{:else if np.album}<span class="album-name">{np.album}</span>{/if}
          </div>
        {:else if isLocal() || live.view}
          <!-- Another player's: said once its state was read, not before the first poll (the owner, 2026-10-24). -->
          <div class="title idle">Nothing is playing</div>
        {/if}
      </div>
      {#if np}
        <i class="sp b"></i>
        <div class="area" class:bare={!(prefs.playerRow && np.source !== 'device')}>
          <!-- The row under the title (the owner, 2026-10-19): the info button (the cover's overview, as a circle on the
               cover) and the facts the user picked in Settings → Display, in short; with none picked, Add leads there.
               Hidden in Settings; none for the device's own music (local mode: nothing known of it). -->
          {#if prefs.playerRow && np.source !== 'device'}
            <div class="facts">
              <button class="info" class:on={overview} aria-label="About this track" aria-pressed={overview} title="About this track" onclick={() => (overview = !overview)}
                ><Icon name="info" size={18} /></button
              >
              {#each rowFacts as f (f.id)}<span class="fact">{f.short}</span>{/each}
              {#if !prefs.playerFacts.length}<button class="fact add" onclick={toRowSettings}><Icon name="plus" size={14} /><span>Add</span></button>{/if}
            </div>
          {/if}
          {#if np.progress}
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
                onpointercancel={letGo}
                onkeydown={seekKey}
              >
                {#if wave && shaped}
                  <Waveform bins={wave} progress={dur ? pos / dur : 0} paused={np.progress.paused} now={nowMark} />
                {:else}
                  <div class="bar"><div class="fill" style:transform="scaleX({dur ? pos / dur : 0})"></div></div>
                {/if}
                <!-- The plain bar's handle (no lens there, the owner 2026-10-25); over a waveform, while held, the lens
                     instead (where it is going). -->
                {#if canSeek && !shaped}<span class="thumb" style:left="{dur ? (pos / dur) * 100 : 0}%"></span>{/if}
                {#if wave && shaped && lens && held !== null && dur}
                  <WaveLens bins={wave} point={pos / dur} now={nowMark} {...lens} time={clockText(pos)} paused={np.progress.paused} />
                {/if}
              </div>
              <span class="t">{dur ? clockText(dur) : '–:––'}</span>
            </div>
          {/if}
          <div class="transport" role="group" aria-label="Player controls">
            <!-- Shuffle, left of previous, where the page holds the queue (here, the app, a Cratarr player): once, no state
                 (the owner, 2026-10-23: a queue always has an order); Undo puts the order before back. -->
            {#if isLocal()}
              <button class="shuffle" aria-label="Shuffle the rest of the queue" onclick={shuffleOnce}><Icon name="shuffle" size={22} /></button>
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
          <!-- Where it plays, and where to play it (the output's sheet): left of the heart (the owner, 2026-10-17), lit
               while it plays on another device, whose name is under it. -->
          <button
            class="act labelled device"
            class:on={!playsHere()}
            aria-haspopup="dialog"
            aria-label={playsHere() ? 'Playing here; play on' : `Playing on ${np.player ?? 'another player'}; play on`}
            onclick={() => (outputOpen = true)}
            ><Icon name="speaker" size={24} />{#if !playsHere() && np.player}<small>{np.player}</small>{/if}</button
          >
          <!-- In the middle (the owner). Always there; until the server says whether it is a favourite, not to be pressed.
               Not in local mode (decision 0052: no hearts for the device's own music). -->
          {#if !localMode()}
          <button
            class="fav"
            class:on={favorite()}
            disabled={pending() || np.favorite === null}
            aria-label={favorite() ? 'Favourite' : 'Not a favourite'}
            aria-pressed={favorite()}
            onclick={toggleFavorite}
            use:longpress={() => (heartMenu = true)}><Icon name="heart" size={26} /></button
          >
          {/if}
          <!-- More music (Radio and Autoplay, decision 0036, amended), where the device button was; only where the page holds the queue.
               A tap opens its sheet; lit while either is on. -->
          {#if isLocal() && !localMode()}
            <button
              class="act radio more"
              class:on={local.radio || local.autoplay !== null}
              aria-label="More music"
              aria-haspopup="dialog"
              aria-pressed={local.radio || local.autoplay !== null}
              onclick={() => (moreOpen = true)}
              ><Icon name={local.autoplay !== null && !local.radio ? 'infinity' : 'radio'} size={24} />{#if local.radio && local.autoplay !== null}<span
                  class="also"
                  aria-hidden="true"><Icon name="infinity" size={12} /></span
                >{/if}</button
            >
          {/if}
          <button class="act labelled queue" aria-label="Queue" aria-expanded={queueOpen} onclick={() => (queueOpen = true)} use:longpress={() => (queueMenuOpen = true)}
            ><Icon name="playlists" size={24} /><small>{queuePlace}</small></button
          >
        </div>
      {/if}
      <i class="sp d"></i>
      <!-- The three bands at the foot, level with the cover's: landscape only, the first to give way. Their room is kept
           while a look is chosen, also while a track's bands load or where it has none, so a new track does not move the
           rest (the owner, 2026-10-24); drawn as silence until then. A tap opens their settings (lib/BandSheet). -->
      {#if np && prefs.playerBands !== 'off'}
        <button class="bands" aria-label="Player visualisation" onclick={() => (bandsOpen = true)}>
          <BandGlass
            spectrum={spectrum ?? SILENT}
            progress={dur ? pos / dur : 0}
            duration={dur ?? 0}
            paused={!!np.progress?.paused}
            look={prefs.playerBands}
            speed={tune.speed}
            window={tune.window}
            cycles={tune.cycles}
            height={tune.height}
            direction={tune.direction}
            tapering={tune.taper}
            fading={tune.fade}
            count={tune.count}
            gap={tune.gap}
            engine={prefs.playerBandsEngine}
          />
        </button>
      {/if}
    </div>
  </header>

  {#if queueMenuOpen}
    <QueueMenu onclose={() => (queueMenuOpen = false)} />
  {/if}
  {#if queueOpen && np}
    <QueueSheet src={src} onclose={() => (queueOpen = false)} />
  {/if}
</div>

{#if menuAt}
  <TrackMenu start={menuStart} onplaylists={place === 'overlay' ? toPlaylistsPage : null} onclose={() => (menuAt = null)} />
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
{#if outputOpen}
  <OutputSheet onclose={() => (outputOpen = false)} />
{/if}
{#if sleepOpen}
  <SleepSheet onclose={() => (sleepOpen = false)} />
{/if}
{#if bandsOpen}
  <BandSheet onclose={() => (bandsOpen = false)} />
{/if}

{#if moreOpen}
  <MoreMusicSheet onclose={() => (moreOpen = false)} />
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

  /* Animations off on this device (Display): the same as the system's reduced motion. */
  :global(:root[data-motion='off']) .player {
    transition: none;
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
     way (drop-*): the large gaps, the large play button, the small gaps, the title's second line, the progress bar.
     (The info chips are gone: the cover's overview tells about the track, the owner 2026-10-19.) */
  header {
    box-sizing: border-box;
    position: relative;
    display: flex;
    flex: 1 1 0;
    flex-direction: column;
    align-items: center;
    min-height: 0;
    padding: 53px 0 env(safe-area-inset-bottom);
    /* Never a scroll bar (the owner, 2026-10-17): what does not fit gives way instead (drop-*). A scroll bar would
       narrow the player, which then fits, losing the bar, and so on many times a second on a desktop. */
    overflow: hidden;
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

  /* Landscape: the title, the extra details, the waveform, the controls and the bottom row spread evenly over the box
     (as tall as the cover), at least 12 px apart (the owner, 2026-10-24: the details too close to the waveform, it too
     close to the controls, they too far from the bottom row). Portrait keeps its spacers. */
  header.landscape .controls {
    justify-content: space-evenly;
    gap: 12px;
  }

  header.landscape .controls > .sp {
    display: none;
  }

  header.landscape .area {
    display: contents;
  }

  /* A little more between the title and the artist and album under it (the owner, 2026-10-24). */
  header.landscape .sub {
    margin-top: 10px;
  }

  /* Spacers: equal (Finamp's were 4 / 4 / 10 / 4 in landscape; the owner, 2026-10-24: too much between the controls
     and the bottom row, relative to the rest); a small minimum keeps the groups apart. */
  .sp {
    display: block;
    flex: 1 1 0;
    min-height: 12px;
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

  /* Without the extra details (the owner, 2026-10-24: off, nothing shifts between tracks, so its own spacing): the
     waveform a little further from the title, as far as the controls are from it. */
  .area.bare {
    gap: 32px;
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
    position: relative;
    container-type: inline-size;
    display: block;
    width: min(78vw, 440px);
    height: min(78vw, 440px);
    padding: 0;
    background: none;
    border: 0;
    cursor: pointer;
    touch-action: none;
  }

  /* The cover (lib/Cover), as Finamp's: radius 8, a soft shadow under it. */
  .cover-btn {
    border-radius: 8px;
    box-shadow: 0 4px 24px rgb(0 0 0 / 0.3);
  }

  .cover-btn > :global(.cover) {
    user-select: none;
    transition: filter 220ms ease-out;
  }

  /* The row under the title: the info button, then the chosen facts as small read-only pills (as the info chips were),
     scrolling sideways when they do not fit. Centred by its ends' auto margins, not justify-content: centred, a row
     that overflows spills left too, out of the scroll's reach (the owner, 2026-10-24: all of them on, some could not be
     reached). */
  .facts {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .facts > :first-child {
    margin-left: auto;
  }

  .facts > :last-child {
    margin-right: auto;
  }

  /* On a big screen (the owner, 2026-10-24: "two rows"): they wrap, centred, instead of scrolling, in a block two rows
     tall whatever the track (no layout shift as tracks change): one row sits in its middle, two fill it. */
  @media (min-width: 900px) {
    .facts {
      flex-wrap: wrap;
      justify-content: center;
      align-content: center;
      row-gap: 8px;
      min-height: 56px;
      overflow: visible;
    }

    .facts > :first-child,
    .facts > :last-child {
      margin: 0;
    }
  }

  .facts .info {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: var(--muted);
    cursor: pointer;
  }

  .facts .info.on {
    background: var(--art-accent, var(--accent));
    color: var(--accent-text);
  }

  .fact {
    flex: none;
    padding: 2px 9px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: var(--muted);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .fact.add {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    border: 0;
    font: inherit;
    font-size: 0.75rem;
    cursor: pointer;
  }

  header.drop-facts .facts {
    display: none;
  }

  /* The album's name opens the album, looking as the text it was. */
  .album-name {
    display: inline;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    letter-spacing: inherit;
    text-align: inherit;
    cursor: pointer;
  }

  /* Under the overview. */
  .cover-btn.blurred > :global(.cover) {
    filter: blur(14px) saturate(1.2);
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

  /* The artist chip, a dot and the album, on one line that shares out its room: each is cut short at its own end (the
     album a button, which a line's own ellipsis would have replaced whole: "…" on the Salon tablet, 2026-10-23). */
  .sub {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    margin-top: 6px;
    color: var(--muted);
    font-size: 1.05rem;
    white-space: nowrap;
  }

  .sub .dot {
    flex: none;
  }

  .sub .album-name {
    flex: 0 1 auto;
    min-width: 3em;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* The artist: a chip in the line (as the panel's artist chip), the album after it as text. */
  .sub .artist {
    display: inline-flex;
    flex: 0 1 auto;
    min-width: 0;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    padding: 1px 10px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 9%, transparent);
    color: var(--text);
    font: inherit;
    vertical-align: baseline;
    cursor: pointer;
  }

  .sub .artist:disabled {
    cursor: default;
  }

  .sub .artist span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sub .liked {
    display: flex;
    flex: none;
  }

  .sub .liked :global(svg) {
    fill: currentColor;
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
  /* Five places, the heart in the middle whatever is shown: menu, device, heart, More music (where the page holds the
     queue), queue. */
  .side {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    align-items: center;
    justify-items: center;
    /* Room for the label under a button (.labelled): counted when the player is fitted, so it is never cut off. */
    padding-bottom: 12px;
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

  .side .more {
    grid-column: 4;
  }

  .side .queue {
    grid-column: 5;
  }

  /* The radio, lit while on (as the heart); the device while it plays elsewhere. */
  .act.radio.on,
  .act.device.on {
    color: var(--art-accent);
  }

  .act.device small {
    max-width: 88px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Its icon follows what is on (the owner): the radio, ∞ for Autoplay alone, the radio with a small ∞ for both. */
  .act.radio {
    position: relative;
  }

  .act.radio .also {
    position: absolute;
    right: 4px;
    bottom: 6px;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg);
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
  /* The three bands: landscape's foot only, and the first thing to give way when the height runs short. */
  .bands {
    display: block;
    width: 100%;
    /* BandGlass's height, kept while it loads. */
    min-height: 132px;
    flex: none;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  header:not(.landscape) .bands,
  header.drop-bands .bands {
    display: none;
  }

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



  header.drop-title .title {
    display: block;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  header.drop-progress .progress {
    display: none;
  }
</style>
