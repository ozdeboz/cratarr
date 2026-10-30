<script lang="ts">
  import DetailsSheet from './DetailsSheet.svelte';
  import VisualsAutoSheet from './VisualsAutoSheet.svelte';
  import { leavePlayer } from './navStep.svelte';
  import StartStopSheet from './StartStopSheet.svelte';
  import LevelSheet from './LevelSheet.svelte';
  import { levelShare, playingLevelOn } from './level.svelte';
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
  import { placeNow } from './trackPlace.svelte';
  import { art, clockText, coverComing, coverFailed, coverLate, coverSrc, shownCover, duration, favorite, paused, pending, playing, shownPlaying, position, seekTo, send, toggleFavorite, playPause, rampBusy, rampInert, shownPaused, wind, windTouch, hasNext, waveStill, playingKey } from './remote.svelte';
  import SeekWave from './SeekWave.svelte';
  import ProgressRing from './ProgressRing.svelte';
  import { Seek } from './seekState.svelte';
  import { heardOf, loadWaveform, waveformOf, waveLoading } from './waveforms.svelte';
  import { SILENT, loadSpectrum, spectrumOf } from './bandWaves.svelte';
  import { tuneOf } from './bandTune.svelte';
  import BandGlass from './BandGlass.svelte';
  import { isLocal, playsHere, shownPlaylists } from './output.svelte';
  import { port } from './player/port.svelte';
  import { ramping } from './ramping.svelte';
  import { appInfo, inApp } from './app.svelte';
  import { startStopNow } from './device.svelte';
  import { holdWind, holdsHere, type Hold } from './holdWind';
  import { local, rewindNow, windDrag, windUpMs } from './localPlayer.svelte';
  import TapeDeck from './TapeDeck.svelte';
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
  import { askedOf, playerShown } from './following.svelte';

  let { place }: { place: 'overlay' | 'side' } = $props();

  // As shown: the new track's words with its cover, one commit (lib/remote shownPlaying).
  const np = $derived(shownPlaying());
  // The tape deck in the row under the title while an effect runs (lib/TapeDeck; the owner, 2026-10-27): a Start and
  // stop ramp or a rewind; kept DECK_HOLD_MS after the last of them ends (the owner, 2026-10-27: the fade only after the
  // last event), a new one starting the count again; then the row back over a second's fade. Or always, where asked.
  const DECK_HOLD_MS = 5000;
  const effecting = $derived(ramping.dir !== null || (isLocal() ? rewindNow.on : !!np?.progress?.rewinding));
  let deckShown = $state(false);
  $effect(() => {
    if (prefs.tapeDeck === 'off') return void (deckShown = false);
    if (prefs.tapeDeck === 'always' || effecting) return void (deckShown = true);
    if (!untrack(() => deckShown)) return;
    const t = setTimeout(() => (deckShown = false), DECK_HOLD_MS);
    return () => clearTimeout(t);
  });

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
  /** Start and stop on for this device's player: a start winds up (the play button's fill stays empty while paused). */
  const armed = $derived(port().ramps && startStopNow().mode !== 'off');
  /** A ramp of this device's player under way, its way. */
  // The fill follows play and pause only, never the tape's own stops (the owner, 2026-10-27).
  const ramp = $derived(port().ramps && ramping.effect ? ramping.dir : null);
  /** Paused with Start and stop on: how much of the wind-up the fill already shows, from where it would start (as it
   *  is held while seeking too): near the track's start the wind-up is shorter (lib/localPlayer windUpMs). */
  const pre = $derived.by(() => {
    const ss = startStopNow();
    // The app's vinyl starts from a quarter of its speed (Android's floor), the browser's from 0.07.
    return ss.ms ? 1 - windUpMs(ss.ms, ss.mode, pos, inApp ? 0.25 : undefined) / ss.ms : 1;
  });
  // A new cover slow to come (the owner, 2026-10-26): after COVER_WAIT_MS the old one fades to dark and the ring fades
  // in over it; the new one takes its place at once when it is in, at full brightness (the owner, 2026-10-26: it faded up
  // from the dark, `coverSlow` let go an effect later than the new cover came). Twice the wait it had (300 ms), so an
  // ordinary load never dims.
  const COVER_WAIT_MS = 600;
  let slowSince = $state(false);
  const coverSlow = $derived(slowSince && coverComing());
  $effect(() => {
    if (!coverComing()) return void (slowSince = false);
    const t = setTimeout(() => (slowSince = true), COVER_WAIT_MS);
    return () => clearTimeout(t);
  });
  // The backdrop list: the current blurred cover, and while it fades in, the one before it.
  let backdrops = $state<{ key: number; src: string }[]>([]);
  let backdropKey = 0;
  const BACKDROP_FADE_MS = 1000;
  $effect(() => {
    const s = src;
    untrack(() => {
      if (!s) backdrops = [];
      else if (backdrops.at(-1)?.src !== s) {
        // The one before stays under it until it is covered (the owner, 2026-10-26: it went at once; an outro of the
        // list's only item did not hold it), then goes.
        const key = ++backdropKey;
        backdrops = [...backdrops.slice(-1), { key, src: s }];
        setTimeout(() => (backdrops = backdrops.filter((b) => b.key >= key)), reducedMotion() ? 0 : BACKDROP_FADE_MS);
      }
    });
  });
  // Finamp's cover: a hard cut when the image is there at once, which it always is here (remote.svelte.ts shows a
  // cover once it has loaded), so it never fades in.
  const dur = $derived(duration());

  // Seeking (decision 0019, now that Cratarr plays music itself): as Finamp's slider, the handle and the time follow
  // the finger and the player seeks only on release. Only on this device for now: seeking the server's player is
  // tried on it first.
  const canSeek = $derived(port().seekable && !!dur);
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
  // Seeking (lib/seekState, the same control as the mini player's, lib/SeekWave): what the bar and the time show.
  const seek = new Seek(position, () => dur, seekTo);
  const pos = $derived(seek.pos);

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
    const q = port().queue();
    if (q) return q.index < 0 ? '' : `${q.index + 1} of ${q.tracks.length}`;
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
    leavePlayer();
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
    leavePlayer();
    location.hash = `#/settings?view=${encodeURIComponent(SETTINGS + 'player')}`;
  }
  /** The album's name: the album in the panel, as the artist's chip opens the artist. */
  function toAlbum() {
    const t = np;
    if (!t?.album_id) return;
    leavePlayer();
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

  /** Holding Previous or Next winds the tape (decision 0074), where that hold is on here (the owner, 2026-10-28: each
   *  motion on its own); another of Cratarr's players decides for itself. */
  const windHold = (dir: 'back' | 'forward'): Hold =>
    holdsHere(dir, np?.player)
      ? {
          onhold: () => wind(dir, 'start'),
          onrelease: () => wind(dir, 'stop'),
          onpress: () => windTouch(dir, true),
          oncancel: () => windTouch(dir, false),
          // Dragged down: held back, to aim (this page's player; the app's and one driven hold for themselves).
          ondrag: port().seekable && !inApp ? windDrag : undefined,
        }
      : null;

  /** What a swipe left or right on the cover was chosen to do: a track on or back, or a seek. */
  function side(what: SwipeSide) {
    if (what === 'next' || what === 'previous') return send(what);
    const at = position();
    seekTo(Math.max(0, what === 'forward' ? Math.min((dur ?? at) - 1, at + SEEK_STEP) : at - SEEK_STEP));
  }
  /** What a tap, a double tap or a circle on the cover was chosen to do. */
  function touch(what: CoverTouch) {
    if (what === 'playPause') playPause();
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
    // Measuring sets the classes and the box's width directly (its height free) and puts them back afterwards; the
    // state then sets the outcome.
    const before = { cls: el.className, width: box.style.width, height: box.style.height };
    // Each orientation's room with its own padding (landscape's differs), whichever is shown now.
    const space = (landscape: boolean) => {
      el.classList.toggle('landscape', landscape);
      const cs = getComputedStyle(el);
      return {
        width: el.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight),
        height: el.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom),
      };
    };
    const room = space(false);
    if (room.width <= 0 || room.height <= 0) {
      el.className = before.cls;
      return;
    }
    const measure = (landscape: boolean, level: number, controlsWidth: number) => {
      el.classList.toggle('landscape', landscape);
      STEPS.forEach((step, i) => el.classList.toggle(`drop-${step}`, level > i));
      box.style.width = `${controlsWidth}px`;
      box.style.height = 'auto';
      return box.getBoundingClientRect().height;
    };
    const next = fit(space, measure);
    el.className = before.cls;
    box.style.width = before.width;
    box.style.height = before.height;
    if (JSON.stringify(next) !== JSON.stringify(layout)) layout = next;
  }

  $effect(() => {
    if (!head) return;
    const ro = new ResizeObserver(() => relayout());
    // Its outer size only: the padding its own layout sets is no resize.
    ro.observe(head, { box: 'border-box' });
    return () => ro.disconnect();
  });
  // What is shown changes the controls' height (a long title, a progress bar or none). As one value: `np` is
  // new at every report, and laying out again at each would be many times a second.
  const shown = $derived(JSON.stringify([np?.id, np?.name, np?.artist, np?.album, np?.player, !!np?.progress, prefs.playerRow, rowFacts.length, prefs.playerBands !== 'off', prefs.tapeDeck !== 'off']));
  $effect(() => {
    void shown;
    untrack(() => requestAnimationFrame(relayout));
  });
  /** A long press on the play button: Start and stop's controls (lib/StartStopSheet; the owner, 2026-10-26). */
  let startStopOpen = $state(false);
  /** A long press on Level: its sheet (lib/LevelSheet). */
  let levelOpen = $state(false);
  /** A long press on the info button: the facts beside it (lib/DetailsSheet; the owner, 2026-10-26). */
  let detailsOpen = $state(false);
  /** A long press on the visuals button: the visuals by themselves (lib/VisualsAutoSheet; the owner, 2026-10-30). */
  let visualsSheet = $state(false);
  /** The Level button's long press: the waveform's three ways in turn, as its sheet lists them (the owner, 2026-10-28). */
  const WAVE_CYCLE = ['file', 'difference', 'output'] as const;
</script>

<div class="player" class:in-sidebar={place === 'side'} style:--art-accent={art.accent ?? 'var(--accent)'}>
  <!-- The blurred cover: a new one fades in over the old one in a second (Finamp's background), the old one staying
       until it is covered. Unless the user said not (Settings › Appearance › Cover colours): the page's own ground. -->
  {#if prefs.coverColourPlayer}
    {#each backdrops as b (b.key)}
      <img class="backdrop" src={b.src} alt="" aria-hidden="true" in:fade={{ duration: reducedMotion() ? 0 : BACKDROP_FADE_MS, easing: linear }} />
    {/each}
  {/if}
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
      {#if np && port().sleeps}
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
        <button class="corner" aria-label="Visuals" aria-haspopup="dialog" onclick={() => (visuals.open = true)} use:longpress={() => (visualsSheet = true)}><Icon name="visuals" size={22} /></button>
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
    <div class="album" class:waiting={coverSlow} style:width={layout ? `${layout.album.width}px` : null} style:height={layout ? `${layout.album.height}px` : null}>
      <!-- the stand-in only when there is no cover to load (lib/Cover), not while one loads -->
      {#if np && (src || !coverSrc() || coverLate())}
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
      <span class="cover-wait" class:on={coverSlow} aria-hidden="true"><ProgressRing progress={null} size={56} spin /></span>
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
          <div class="title"><span>{np.name}</span></div>
          <div class="sub">
            {#if np.artist}<button class="artist" disabled={!by} onclick={() => by && goToArtist(np.source, by)}
                ><span>{np.artist}</span>{#if byFavorite}<i class="liked" role="img" aria-label="Favourite"><Icon name="heart" size={14} /></i>{/if}</button
              >{/if}{#if np.artist && np.album}<span class="dot">·</span>{/if}{#if np.album && np.album_id}<button class="album-name" onclick={toAlbum}>{np.album}</button
              >{:else if np.album}<span class="album-name">{np.album}</span>{/if}
          </div>
        {:else if port().holdsQueue || live.view}
          <!-- Another player's: said once its state was read, not before the first poll (the owner, 2026-10-24). -->
          <div class="title idle"><span>Nothing is playing</span></div>
        {/if}
      </div>
      {#if np}
        <i class="sp b"></i>
        <div class="area" class:bare={!(prefs.playerRow && np.source !== 'device')}>
          <!-- The row under the title (the owner, 2026-10-19): the info button (the cover's overview, as a circle on the
               cover) and the facts the user picked in Settings → Display, in short; with none picked, Add leads there.
               Hidden in Settings; none for the device's own music (local mode: nothing known of it). -->
          <div class="under" class:deckroom={prefs.tapeDeck !== 'off'}>
          {#if prefs.playerRow && np.source !== 'device'}
            <div class="facts" class:decked={deckShown}>
              <button class="info" class:on={overview} aria-label="About this track" aria-pressed={overview} title="About this track" onclick={() => (overview = !overview)}
                use:longpress={() => (detailsOpen = true)}><Icon name="info" size={18} /></button
              >
              {#each rowFacts as f (f.id)}<span class="fact">{f.short}</span>{/each}
              {#if !prefs.playerFacts.length}<button class="fact add" onclick={toRowSettings}><Icon name="plus" size={14} /><span>Add</span></button>{/if}
            </div>
          {/if}
          {#if deckShown}<div class="deck-row" class:over={prefs.playerRow && np.source !== 'device'} in:fade={{ duration: 150 }} out:fade={{ duration: 1000 }}><TapeDeck /></div>{/if}
          </div>
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
              <SeekWave {seek} bins={wave} loading={np ? waveLoading(np.source, np.id) : null} {dur} {canSeek} paused={waveStill(np.progress)} estimated={np.progress.estimated} room={head} track={playingKey(playing())} />
              <span class="t">{dur ? clockText(dur) : '–:––'}</span>
            </div>
          {/if}
          <div class="transport" role="group" aria-label="Player controls">
            <!-- Level, left of previous, where the page holds the queue (the owner, 2026-10-26, 0.9.8: in Shuffle's place;
                 Shuffle is in the queue's sheet): on, it fills from the bottom by how far the playing track is turned down
                 (shown, never a number). A tap opens its sheet, where it is switched (the owner, 2026-10-27: not by a
                 tap); a long press shows or hides the levelling in the waveform. -->
            {#if port().level()}
              <button
                class="level"
                class:on={playingLevelOn()}
                aria-label={playingLevelOn() ? 'Level the volume: on' : 'Level the volume: off'}
                title="Level the volume"
                aria-haspopup="dialog"
                onclick={() => (levelOpen = true)}
                use:longpress={() => (prefs.waveLevel = WAVE_CYCLE[(WAVE_CYCLE.indexOf(prefs.waveLevel) + 1) % WAVE_CYCLE.length])}
                ><Icon name="audioLines" size={22} />{#if playingLevelOn()}<span class="fill" style:--down={levelShare(local.levelDb)} aria-hidden="true"
                    ><Icon name="audioLines" size={22} /></span
                  >{/if}</button
              >
            {/if}
            <button aria-label="Previous track" onclick={() => send('previous')} use:holdWind={windHold('back')}><Icon name="previous" size={24} /></button>
            <!-- Its fill (the owner, 2026-10-26), inside a border of the same colour: winding down (Start and stop) it drains
                 from the top over the ramp; paused with Start and stop on it stays empty (a start winds up); winding up it
                 fills from the bottom over the ramp, the way it went. -->
            <button class="main" aria-label={shownPaused() ? 'Play' : 'Pause'} aria-disabled={rampInert()} onclick={playPause} use:longpress={() => (startStopOpen = true)}
              >{#key `${ramp}:${ramping.at}`}<span
                  class="fillbg"
                  class:drain={ramp === 'out'}
                  class:fillup={ramp === 'in'}
                  class:empty={ramp === null && armed && paused()}
                  style:--pre={ramp === 'in' ? 1 - ramping.ms / Math.max(1, ramping.full) : armed && paused() ? pre : 0}
                  style:--from={ramp === 'out' ? Math.min(1, ramping.ms / Math.max(1, ramping.full)) : 1}
                  style:animation-duration={ramp ? `${ramping.ms}ms` : null}
                ></span>{/key}<Icon
                name={shownPaused() ? 'play' : 'pause'}
                size={32}
              /></button
            >
            <!-- Dimmed where nothing can follow (no next track, no Autoplay, Radio or repeat to make one). -->
            <button aria-label="Next track" disabled={!hasNext()} onclick={() => send('next')} use:holdWind={hasNext() ? windHold('forward') : null}><Icon name="next" size={24} /></button>
            <!-- Repeat is in the queue's sheet (a long press on Queue; the owner, 2026-10-26, 0.9.8): its place kept, so
                 play stays in the middle. -->
            {#if port().radio}<span class="spot" aria-hidden="true"></span>{/if}
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
          {#if !localMode() && np.source === BANDCAMP}
            <!-- A Bandcamp track: no heart, crossed out and said so (the owner, 2026-10-26). -->
            <span class="fav off" role="img" aria-label="On Bandcamp" title="On Bandcamp"><Icon name="heartOff" size={26} /></span>
          {:else if !localMode()}
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
          {#if port().radio && !localMode()}
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
            ><Icon name="playlists" size={24} /><small>{queuePlace}</small>{#if askedOf(playerShown())}<span class="asked-count">{askedOf(playerShown())}</span>{/if}</button
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
            clock={placeNow}
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


{#if startStopOpen}<StartStopSheet onclose={() => (startStopOpen = false)} />{/if}
{#if levelOpen}<LevelSheet onclose={() => (levelOpen = false)} />{/if}
{#if detailsOpen}<DetailsSheet onclose={() => (detailsOpen = false)} />{/if}
{#if visualsSheet}<VisualsAutoSheet onclose={() => (visualsSheet = false)} />{/if}

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
    position: relative;
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
    transition: opacity 1s;
  }

  /* The row and the tape deck in one place: the deck over the row while an effect runs, the row back as it fades. */
  .under {
    position: relative;
    display: grid;
    align-items: center;
  }

  /* Over the row it takes the row's place without its height (the owner, 2026-10-27: no layout shift): centred on it,
     reaching into the room above and below. */
  .deck-row.over {
    position: absolute;
    top: 50%;
    right: 0;
    left: 0;
    transform: translateY(-50%);
  }

  .under > * {
    grid-area: 1 / 1;
  }

  /* Where the deck can show, the row keeps its height always (the owner's tablet, 2026-10-28: a one-line row of 26 px,
     the deck 43 over it, lay on the artist's line and the waveform): never a shift as it comes, and the layout measures
     it, the cover giving way. */
  .under.deckroom {
    /* The deck's 43 px, and a little air above and below it (the small gaps give way first on a short screen). */
    min-height: 49px;
  }

  /* Kept in its place, unseen (not `.away`: app.css hides the children of that, and the row would collapse). */
  .facts.decked {
    opacity: 0;
    pointer-events: none;
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

  /* Always two lines' room, a one-line title in their middle (the owner, 2026-10-26: no layout shift from track to
     track). */
  .title {
    display: flex;
    align-items: center;
    height: 2.5em;
    font-size: 1.6rem;
    font-weight: 650;
    line-height: 1.25;
  }

  .title > span {
    display: -webkit-box;
    flex: 1;
    min-width: 0;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
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

  /* A new cover slow to come: the old one darkens and the ring comes in over its middle, both over 400 ms. */
  .album .cover-btn {
    transition: filter 400ms ease-out;
  }

  .album.waiting .cover-btn {
    filter: brightness(0.3);
  }

  .cover-wait {
    position: absolute;
    inset: 0;
    opacity: 0;
    pointer-events: none;
    transition: opacity 400ms ease-out;
  }

  .cover-wait.on {
    opacity: 1;
  }

  .fav.off {
    display: grid;
    place-items: center;
    color: var(--muted);
    cursor: default;
  }

  /* The seek bar's own (its waveform, handle and lens): lib/SeekWave. */

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

  /* Level: muted while off; on, in the text colour, the accent filling it from the bottom by how far the track is
     turned down. */
  .transport button.level {
    position: relative;
    color: var(--muted);
  }

  .transport button.level.on {
    color: var(--text);
  }

  .level .fill {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--art-accent, var(--accent));
    clip-path: inset(calc((1 - var(--down)) * 100%) 0 0 0);
    transition: clip-path 400ms ease-out;
  }

  /* Repeat's place, kept empty (it is in the queue's sheet): a button's width. */
  .transport span.spot {
    width: 48px;
    height: 48px;
  }

  /* Play/pause in Finamp's shape: a rounded square (a quarter of its size as radius), tinted with the text colour,
     its icon lifted by a soft shadow (not on a weak chip, below). Previous and next are plain icons. */
  .transport button.main {
    position: relative;
    width: 62px;
    height: 62px;
    border-radius: 16px;
    background: transparent;
    /* Its border, the fill's colour, outside it (the owner, 2026-10-26: border and fill never overlap). */
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--text) 15%, transparent);
  }

  .transport button.main :global(svg) {
    position: relative;
  }

  .fillbg {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: color-mix(in srgb, var(--text) 15%, transparent);
    pointer-events: none;
  }

  /* Paused with Start and stop on: empty, but for the part of the wind-up there is no track for. */
  .fillbg.empty {
    clip-path: inset(calc((1 - var(--pre, 0)) * 100%) 0 0 0);
  }

  /* Evenly over the ramp's time: draining from the top, or filling from the bottom. */
  .fillbg.drain {
    animation: drain linear forwards;
  }

  .fillbg.fillup {
    animation: fillup linear forwards;
  }

  /* From as full as the ramp it turned round had got (a pause during a wind-up: only what is left of the ramp). */
  @keyframes drain {
    from {
      clip-path: inset(calc((1 - var(--from, 1)) * 100%) 0 0 0);
    }
    to {
      clip-path: inset(100% 0 0 0);
    }
  }

  @keyframes fillup {
    from {
      clip-path: inset(calc((1 - var(--pre, 0)) * 100%) 0 0 0);
    }
    to {
      clip-path: inset(0 0 0 0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .fillbg.drain,
    .fillbg.fillup {
      animation-timing-function: steps(4, end);
    }
  }

  /* Its icon lifted by a soft shadow, but not on a weak graphics chip (lib/gpu: the tablet's Mali drew specks around it). */
  :global(:root:not([data-gpu='lite'])) .transport button.main :global(svg) {
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

  /* Capped, so centred in its wider box by its margins (pinned to the box's left, it sat off the icon; the owner's
     tablet, 2026-10-27). */
  .act.device small {
    max-width: 88px;
    margin-inline: auto;
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

  /* Centred by a box wider than the button, not by a half-width shift: a shift by a fraction of a pixel had the tablet's
     Mali draw specks around it (the owner's tablet, 2026-10-26, as the play icon's drop-shadow did; lib/gpu). */
  .act.labelled small {
    position: absolute;
    top: calc(100% - 4px);
    left: -40px;
    right: -40px;
    text-align: center;
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
    height: 1.25em;
  }

  header.drop-title .title > span {
    display: block;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  header.drop-progress .progress {
    display: none;
  }

  /* Tracks asked for of this player (0.10): how many, on the queue button's corner. */
  .asked-count {
    position: absolute;
    top: -4px;
    right: -6px;
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
