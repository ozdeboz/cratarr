<script lang="ts">
  import Playlists from './Playlists.svelte';
  import Health from './Health.svelte';
  import Settings from './Settings.svelte';
  import Library from './Library.svelte';
  import Home from './Home.svelte';
  import LocalAlbums from './LocalAlbums.svelte';
  import LocalArtists from './LocalArtists.svelte';
  import LocalTracks from './LocalTracks.svelte';
  import ModeSwitcher from './lib/ModeSwitcher.svelte';
  import { localMode, switchable } from './lib/deviceMusic.svelte';
  import Insights from './Insights.svelte';
  import Downloads from './Downloads.svelte';
  import SettingsPanel from './lib/SettingsPanel.svelte';
  import { SECTIONS, type SettingsSection } from './lib/settingsSections.svelte';
  import { askDownloads, downloads, inApp } from './lib/app.svelte';
  import { reach } from './lib/reach.svelte';
  import { keepCoversAway } from './lib/coversAway.svelte';
  import Visuals from './lib/visuals/Visuals.svelte';
  import { visuals } from './lib/visuals/shown.svelte';
  import Crates from './Crates.svelte';
  import AboutSheet from './lib/AboutSheet.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import { target } from './lib/playOnTarget.svelte';
  import { listening, resumeGroup } from './lib/groupListen.svelte';
  import { startWith, togglePlay } from './lib/localPlayer.svelte';
  import NowPlaying from './lib/NowPlaying.svelte';
  import SheetArea from './lib/SheetArea.svelte';
  import MiniPlayer from './lib/MiniPlayer.svelte';
  import TopPill from './lib/TopPill.svelte';
  import InsightsTabs from './lib/InsightsTabs.svelte';
  import InsightsPanel from './lib/InsightsPanel.svelte';
  import Mark from './lib/Mark.svelte';
  import LabelTip from './lib/LabelTip.svelte';
  import { installLabels } from './lib/labels.svelte';
  import { hasView, pageView } from './lib/pageView.svelte';
  import { art, playing } from './lib/remote.svelte';
  import { fade, fly } from 'svelte/transition';
  import { linear, quadInOut } from 'svelte/easing';
  import { reducedMotion } from './lib/motion';
  import Toasts from './lib/Toasts.svelte';
  import { toast } from './lib/toast.svelte';
  import ProgressBar from './lib/ProgressBar.svelte';
  import { untrack } from 'svelte';
  import { inSidebar, live, panelShowsPlayer, player } from './lib/live.svelte';
  import { closePlaylist, INSIGHTS as INSIGHTS_VIEW, keepViewed, panel, SETTINGS as SETTINGS_VIEW, settingsOf, viewInPlace, viewPlaylist } from './lib/panel.svelte';
  import PlaylistView from './lib/PlaylistView.svelte';
  import { poll } from './lib/live.svelte';
  import { known, remember } from './lib/sources.svelte';
  import { getStatus, type Status } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { jobs } from './lib/jobs.svelte';
  import { loadMe, me } from './lib/me.svelte';
  import { loadMine } from './lib/mySettings.svelte';

  type Page = { id: string; href: string; label: string; icon: IconName };
  // Main pages in order; Insights, Health and Settings sit apart at the bottom of the sidebar, in that order (decision
  // 0025: Health beside Settings, Insights beside Health).
  const PLAYLISTS: Page = { id: 'playlists', href: '#/', label: 'Playlists', icon: 'playlists' };
  // The quick look, the leftmost tab (the owner, 2026-10-23); where the app opens without an address.
  const HOME: Page = { id: 'home', href: '#/home', label: 'Home', icon: 'home' };
  // Every used library's albums, Music and Incoming (decision 0034: one page; a dashboard will give the quick look).
  const LIBRARY: Page = { id: 'library', href: '#/library', label: 'Library', icon: 'library' };
  // Incoming's and Collection's addresses from before Library (decision 0034): they open Library.
  const FORMER = ['#/incoming', '#/collection'];
  const INSIGHTS: Page = { id: 'insights', href: '#/insights', label: 'Insights', icon: 'insights' };
  // The second step after the Library (decision 0024).
  const CRATES: Page = { id: 'crates', href: '#/crates', label: 'Crates', icon: 'crates' };
  const HEALTH: Page = { id: 'health', href: '#/health', label: 'Health', icon: 'health' };
  const SETTINGS: Page = { id: 'settings', href: '#/settings', label: 'Settings', icon: 'settings' };
  // In the app only: what is kept on this device (decision 0033, step 4), playable without a connection.
  const DOWNLOADS: Page = { id: 'downloads', href: '#/downloads', label: 'Downloads', icon: 'download' };
  // Local mode's (decision 0052): the device's own music, and the menu shows only these (and Settings).
  const LOCAL_ALBUMS: Page = { id: 'local-albums', href: '#/local/albums', label: 'Albums', icon: 'disc' };
  const LOCAL_ARTISTS: Page = { id: 'local-artists', href: '#/local/artists', label: 'Artists', icon: 'users' };
  const LOCAL_TRACKS: Page = { id: 'local-tracks', href: '#/local/tracks', label: 'Tracks', icon: 'music' };
  const LOCAL = [LOCAL_ALBUMS, LOCAL_ARTISTS, LOCAL_TRACKS];

  // Phones get a bottom bar. Up to five fit; beyond that the first four stay and the rest go under "More".
  const BAR_SLOTS = 5;

  // Playlists are created, renamed, deleted and repaired on the Playlists page and its panel now; old Manage and
  // Repairs links land there (decision 0025). The page is the address before `?` (what is viewed rides along as
  // `?view=`, decision 0023).
  const fromHash = () => {
    const [h, rest] = location.hash.split('?');
    if (FORMER.includes(h)) {
      history.replaceState(history.state, '', LIBRARY.href + (rest ? `?${rest}` : ''));
      return LIBRARY.id;
    }
    if (!h) return HOME.id;
    return [HOME, HEALTH, SETTINGS, LIBRARY, INSIGHTS, CRATES, DOWNLOADS, ...LOCAL].find((p) => p.href === h)?.id ?? PLAYLISTS.id;
  };
  let page = $state(fromHash());
  // Where the player shows (decision 0018): beside the page on a roomy window while something plays (`sidebar`);
  // otherwise the mini-player along the bottom (`mini`), which opens it over everything (`overlay`). The player over
  // everything closes when the page changes or the navigation is used, and the page behind it does not scroll.
  const collapse = () => (player.full = false);
  // The navigation folds to a strip of icons on a wide window (decision 0042). Folded, it shows whole over the page,
  // never pushing it aside, while a mouse is on it or after a tap on the strip outside its icons; a tap elsewhere or
  // the mouse leaving folds it again.
  let peek = $state(false);
  $effect(() => {
    if (!peek) return;
    const away = (e: PointerEvent) => {
      if (!(e.target as Element | null)?.closest('.nav-col')) peek = false;
    };
    addEventListener('pointerdown', away, true);
    return () => removeEventListener('pointerdown', away, true);
  });
  function stripTap(e: MouseEvent) {
    if (prefs.navFolded && !peek && !(e.target as Element).closest('a, button')) peek = true;
  }
  // Its edge is a handle, as the panel's (decision 0042): a tap folds or unfolds it; a drag follows the pointer and
  // snaps on release to whichever of the two widths is nearer, never anything between.
  const NAV_OPEN = 240;
  const NAV_FOLDED = 68;
  let navDrag = $state<number | null>(null);
  let navDragged = false;
  function navGrip(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const x0 = e.clientX;
    const w0 = prefs.navFolded && !peek ? NAV_FOLDED : NAV_OPEN;
    navDragged = false;
    const move = (m: PointerEvent) => {
      if (Math.abs(m.clientX - x0) > 4) navDragged = true;
      if (navDragged) navDrag = Math.min(NAV_OPEN, Math.max(NAV_FOLDED, w0 + m.clientX - x0));
    };
    const up = () => {
      if (navDragged && navDrag !== null) prefs.navFolded = navDrag < (NAV_OPEN + NAV_FOLDED) / 2;
      navDrag = null;
      peek = false;
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      el.removeEventListener('pointercancel', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
    el.addEventListener('pointercancel', up);
  }
  function navTap() {
    // The click that ends a drag is not a tap.
    if (navDragged) {
      navDragged = false;
      return;
    }
    prefs.navFolded = !(prefs.navFolded && !peek);
    peek = false;
  }
  const navFoldedNow = $derived(navDrag !== null ? navDrag < (NAV_OPEN + NAV_FOLDED) / 2 : prefs.navFolded);
  $effect(() => {
    page;
    untrack(collapse);
  });
  // Settings on a roomy window is its list beside a part (the owner, 2026-10-19: "just force the panel open on the
  // settings page"): the panel is always beside it there, showing a part.
  const settingsSplit = $derived(page === 'settings' && player.roomy);
  // Insights' own panel (lib/InsightsPanel), when it shows: on a roomy window always beside the page, at a fixed width
  // (the owner, 2026-10-23: never full screen), whatever the place chosen for the others.
  const insightsShown = $derived(panel.viewed === INSIGHTS_VIEW && !panel.playerTop);
  const insightsSplit = $derived(insightsShown && player.roomy);
  const sidebar = $derived(settingsSplit || insightsSplit || inSidebar());
  // The visuals beside the page: in the panel where the player goes, while it is there (a narrow window: over the page).
  const visualsBeside = $derived(visuals.open && visuals.side && sidebar);
  const overlay = $derived(player.full && !sidebar);
  // The mini-player stands in for the player wherever the panel does not show it (decision 0023); with nothing to play
  // it is the small button for where music plays.
  const mini = $derived(!(sidebar && panelShowsPlayer()));
  // A viewed playlist without room beside the page: a layer of its own, under the mini-player and the player.
  const playlistLayer = $derived(!sidebar && panel.viewed !== null);
  $effect(() => {
    if (sidebar) untrack(collapse);
  });
  $effect(() => {
    document.documentElement.style.overflow = overlay || playlistLayer ? 'hidden' : '';
  });
  // Same breakpoint as the CSS: from 1024 px the sidebar shows; below it the bar of pages along the top.
  const wide = window.matchMedia('(min-width: 1024px)');
  let phone = $state(!wide.matches);
  $effect(() => {
    const on = () => (phone = !wide.matches);
    wide.addEventListener('change', on);
    return () => wide.removeEventListener('change', on);
  });
  let moreOpen = $state(false);

  // Room for the player beside the page (decision 0018): a content area of at least 800 × 500 px, the page keeping
  // 400 px and the player 275 px. Its width is dragged at its edge and remembered.
  const NAV_WIDTH = 240;
  const TOP_BAR = 65;
  /** The phone's slim row of the mode switcher over the tabs (decision 0052, mock C), while it shows. */
  const SLIM = 34;
  const topBar = $derived(TOP_BAR + (switchable() ? SLIM : 0));
  /** The mini-player's height with its margin, kept free under the page and the toasts. */
  const MINI_SPACE = 88;
  const miniSpace = $derived(mini && playing() ? MINI_SPACE : 0);
  // The page's row (Symfonium's, decided 2026-10-15), floating over the page's top: back on the left, when this visit
  // has a step before this one; on the right, on a page with something to play, the pill (lib/TopPill): the filter
  // always, the page's View, and where it plays when nothing else leads there (no mini-player, no player beside the
  // page). The page starts under the row, so at rest nothing lies under it.
  const PLAYABLE = ['playlists', 'crates', 'library', 'downloads', 'local-albums', 'local-artists', 'local-tracks'];
  // A part of the settings in the panel (lib/SettingsPanel), when the panel shows one.
  const settingsShown = $derived(SECTIONS.find((x) => x.id === settingsOf(panel.viewed))?.id ?? (null as SettingsSection | null));
  // Beside Settings' list a part is always open: the one last open in this browser, else the first; opened in place, so
  // Back leaves the page rather than closing it. Once it is known who is signed in (the server's parts are its
  // administrators'). Only while the address is Settings' own: leaving it (Back, a link) clears the part a moment before
  // `page` follows, and opening one then would write it into the next page's address.
  $effect(() => {
    if (!settingsSplit || settingsShown || !me.known || fromHash() !== 'settings') return;
    const parts = SECTIONS.filter((x) => (!x.app || inApp) && (!x.admin || me.admin));
    const part = parts.find((x) => x.id === prefs.settingsPart) ?? parts[0];
    if (part) untrack(() => viewInPlace(SETTINGS_VIEW + part.id));
  });
  $effect(() => {
    if (settingsShown) prefs.settingsPart = settingsShown;
  });
  const pagePill = $derived(PLAYABLE.includes(page));
  // An icon button's label on a long-press or right-click, everywhere (lib/labels).
  $effect(installLabels);
  // Each page starts with its search empty (the pill's field).
  $effect(() => {
    void page;
    pageView.query = '';
  });
  // The page's device button gives way to the panel's (the owner, 2026-10-16): a list beside the page has its own.
  const panelDevice = $derived(sidebar && panel.viewed !== null && !panelShowsPlayer() && !settingsShown && !insightsShown);
  const pillDevice = $derived(!playing() && !(sidebar && panelShowsPlayer()) && !panelDevice);
  // Each step of this visit carries its place (`nav` in its history state): back shows from the second step on, never on
  // the one the app opened with.
  let navAt = $state(0);
  let lastNav = -1;
  function stamp() {
    const st = history.state ?? {};
    if (typeof st.nav !== 'number') history.replaceState({ ...st, nav: lastNav + 1 }, '');
    navAt = history.state.nav;
    lastNav = navAt;
  }
  $effect(() => {
    stamp();
    addEventListener('hashchange', stamp);
    addEventListener('popstate', stamp);
    return () => {
      removeEventListener('hashchange', stamp);
      removeEventListener('popstate', stamp);
    };
  });
  // On a phone the row always has something on its left (back, or the mark); beside the page only back or the pill.
  const pageRow = $derived(!playlistLayer && !overlay && (phone || navAt > 0 || pagePill || page === 'settings' || page === 'insights'));
  // The settings page's info button (the owner, 2026-10-16): About in a sheet.
  let aboutOpen = $state(false);
  $effect(() => {
    const root = document.documentElement.style;
    root.setProperty('--top-h', phone ? `${topBar}px` : '0px');
    root.setProperty('--bottom-h', `${miniSpace}px`);
  });
  // The playing cover's colour, for the floating controls' surface (app.css --float-bg): the mini-player's.
  $effect(() => {
    document.documentElement.style.setProperty('--now-accent', art.accent ?? 'var(--accent)');
  });
  // The window's height as it is shown (--app-h), for the columns that are as tall as the window: an installed app on
  // Android counts more than the visible window in 100dvh (the sidebar's end went under the screen's edge, 2026-10-14).
  $effect(() => {
    const on = () => document.documentElement.style.setProperty('--app-h', `${innerHeight}px`);
    on();
    addEventListener('resize', on);
    return () => removeEventListener('resize', on);
  });
  let area = $state({ w: 0, h: 0 });
  $effect(() => {
    const on = () => (area = { w: innerWidth - (phone ? 0 : NAV_WIDTH), h: innerHeight - (phone ? topBar : 0) });
    on();
    addEventListener('resize', on);
    return () => removeEventListener('resize', on);
  });
  $effect(() => {
    player.roomy = area.w >= 800 && area.h >= 500;
  });
  const PLAYER_MIN = 275;
  const PAGE_MIN = 400;
  const fitWidth = (w: number) => Math.round(Math.min(Math.max(w, PLAYER_MIN), area.w - PAGE_MIN));
  let dragged = $state<number | null>(null);
  const playerWidth = $derived(fitWidth(dragged ?? prefs.playerWidth));
  // Beside Settings' list the part takes all but the list's column (no grip there: the list's width is its own).
  const SETTINGS_LIST = 400;
  const INSIGHTS_PANEL = 360;
  const sideWidth = $derived(settingsSplit ? Math.max(PLAYER_MIN, area.w - SETTINGS_LIST) : insightsSplit ? INSIGHTS_PANEL : playerWidth);
  let contentEl = $state<HTMLElement>();
  // The page's side and the panel's: sheets opened in either stand over it (lib/SheetArea.svelte).
  let pageEl = $state<HTMLElement>();
  let sideEl = $state<HTMLElement>();
  function grip(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const right = contentEl!.getBoundingClientRect().right;
    const move = (m: PointerEvent) => (dragged = fitWidth(right - m.clientX));
    const up = () => {
      if (dragged !== null) prefs.playerWidth = dragged;
      dragged = null;
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      el.removeEventListener('pointercancel', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
    el.addEventListener('pointercancel', up);
  }
  function gripKey(e: KeyboardEvent) {
    const step = e.key === 'ArrowLeft' ? 16 : e.key === 'ArrowRight' ? -16 : 0;
    if (!step) return;
    e.preventDefault();
    prefs.playerWidth = fitWidth(playerWidth + step);
  }

  $effect(() => {
    const on = () => {
      page = fromHash();
      moreOpen = false;
    };
    addEventListener('hashchange', on);
    return () => removeEventListener('hashchange', on);
  });

  $effect(() => {
    if (!moreOpen) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (moreOpen = false);
    addEventListener('keydown', esc);
    return () => removeEventListener('keydown', esc);
  });

  // In the app, Cratarr out of reach before it ever answered (away from home, say): it opens on what plays without it,
  // its downloads (decision 0033, step 4), unless it was opened on a page or a view of its own.
  let openedAway = false;
  $effect(() => {
    if (!inApp || openedAway || reach.answered || !reach.away) return;
    if (!downloads.known) return askDownloads();
    openedAway = true;
    if (downloads.tracks.length && (page === HOME.id || page === PLAYLISTS.id) && !location.hash.replace(/^#\/?/, '')) {
      history.replaceState(history.state, '', DOWNLOADS.href);
      page = DOWNLOADS.id;
    }
  });

  // In the app: the downloaded albums' covers, kept for away from home (lib/coversAway).
  $effect(keepCoversAway);

  // What is playing, for the header above every page; restarts when the followed source changes.
  $effect(() => {
    void prefs.source;
    return poll();
  });

  // Who is signed in (decision 0051): the administrators' parts show for them only. Their own choices (Radio's,
  // Autoplay's) for this page's player while nothing plays (in the app, the app's player keeps its own).
  loadMe();
  loadMine().then((c) => c && !inApp && startWith(c));

  // The conversion queue, followed on every page (an administrator's): its count on the Incoming tab, a toast when an
  // album is done.
  $effect(() => (me.admin ? jobs.start() : undefined));
  const waiting = $derived(jobs.open.length);

  // Connection line in the sidebar header.
  let status = $state<Status | null>(null);
  let statusFailed = $state(false);
  let toldProblem: string | null = null;
  $effect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        status = await getStatus();
        if (!known.list.length) remember(status.sources);
        statusFailed = false;
        // A server problem is said once, in a toast, when it first shows (the sidebar keeps showing it).
        const problem = status.problems[0] ?? null;
        if (problem && problem !== toldProblem) toast.show({ text: problem, tone: 'error' });
        toldProblem = problem;
      } catch {
        statusFailed = true;
      }
      if (!stopped) timer = setTimeout(poll, 30_000);
    }
    poll();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });

  const connection = $derived(
    statusFailed
      ? { ok: false, text: 'Cratarr server unreachable' }
      : !status
        ? null
        : status.connected
          ? status.problems.length
            ? { ok: false, text: status.problems[0] }
            : { ok: true, text: `Connected to ${status.server_name ?? 'Jellyfin'}` }
          : { ok: false, text: 'Jellyfin unreachable' },
  );
  const MAIN: Page[] = $derived(localMode() ? LOCAL : [HOME, LIBRARY, CRATES, PLAYLISTS]);
  // Apart, at the bottom of the sidebar: Insights beside Health, then Settings.
  // Health is the library's, so its administrators' (decision 0051). In Local mode, Settings only.
  const APART: Page[] = $derived(localMode() ? [SETTINGS] : [...(inApp ? [DOWNLOADS] : []), INSIGHTS, ...(me.admin ? [HEALTH] : []), SETTINGS]);
  // A switch of the mode (decision 0052) goes to the first page of the mode it went to, unless the page is one both
  // have (Settings).
  $effect(() => {
    const local = localMode();
    // No visuals in local mode (the owner: "In local mode, the visuals feature is not supported").
    if (local) untrack(() => (visuals.open = false));
    const p = untrack(() => page);
    if (p === 'settings') return;
    if (local && !p.startsWith('local-')) location.hash = LOCAL_ALBUMS.href;
    else if (!local && p.startsWith('local-')) location.hash = LIBRARY.href;
  });
  const ALL = $derived([...MAIN, ...APART]);
  const bar = $derived(ALL.length <= BAR_SLOTS ? ALL : ALL.slice(0, BAR_SLOTS - 1));
  const overflow = $derived(ALL.filter((p) => !bar.includes(p)));
  const overflowActive = $derived(overflow.some((p) => p.id === page));
</script>

<div class="shell" class:folded={navFoldedNow} style:--nav-w={navDrag !== null ? `${navDrag}px` : undefined}>
  <!-- The navigation's column: the sidebar, which scrolls (and so would cut off anything past its edge), and the handle
       on its edge. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="nav-col"
    class:peek
    onclickcapture={collapse}
    onclick={stripTap}
    onpointerenter={(e) => prefs.navFolded && navDrag === null && e.pointerType === 'mouse' && (peek = true)}
    onpointerleave={(e) => e.pointerType === 'mouse' && navDrag === null && (peek = false)}
  >
    <aside class="sidebar" class:peek>
      <!-- The mode switcher with a local folder on (decision 0052, mock B); otherwise the mark and the connection line. -->
      {#if switchable()}
        <ModeSwitcher {connection} />
      {:else}
        <div class="brand">
          <span class="logo"><Mark /></span>
          <span class="name">Cratarr</span>
        </div>
        {#if connection}
          <a class="conn" class:down={!connection.ok} href={keepViewed("#/settings")}><i aria-hidden="true"></i><span class="label">{connection.text}</span></a>
        {/if}
      {/if}

      <nav aria-label="Pages">
        {#each MAIN as p (p.id)}
          <a href={keepViewed(p.href)} aria-current={page === p.id ? 'page' : undefined} onclick={() => (peek = false)}
            ><Icon name={p.icon} /><span class="label">{p.label}</span>{#if p.id === 'library' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</a
          >
        {/each}
      </nav>

      <div class="foot">
        {#each APART as p (p.id)}
          <a href={keepViewed(p.href)} aria-current={page === p.id ? 'page' : undefined} onclick={() => (peek = false)}><Icon name={p.icon} /><span class="label">{p.label}</span></a>
        {/each}
        {#if status}<div class="version">Cratarr {status.version}</div>{/if}
      </div>
    </aside>
    <button
      class="nav-grip edge-grip"
      aria-label={prefs.navFolded && !peek ? 'Unfold the navigation' : 'Fold the navigation'}
      aria-expanded={!(prefs.navFolded && !peek)}
      title={prefs.navFolded && !peek ? 'Unfold the navigation' : 'Fold the navigation'}
      onpointerdown={navGrip}
      onclick={navTap}
    ></button>
  </div>

  <div class="content" class:with-side={sidebar} style:--player-w="{sideWidth}px" bind:this={contentEl}>
    <div class="page" class:under-row={pageRow} bind:this={pageEl}>
    <SheetArea area={() => pageEl}>
    {#if page === 'settings'}
      <Settings />
    {:else if page === 'health' && me.admin}
      <Health />
    {:else if page === 'downloads' && inApp}
      <Downloads />
    {:else if page === 'library'}
      <Library />
    {:else if page === 'local-albums'}
      <LocalAlbums />
    {:else if page === 'local-artists'}
      <LocalArtists />
    {:else if page === 'local-tracks'}
      <LocalTracks />
    {:else if status && !status.music && me.admin}
      <p class="no-music">No Music library is set. Choose one in Settings → Libraries.</p>
    {:else if page === 'insights'}
      <Insights />
    {:else if page === 'crates'}
      <Crates />
    {:else if page === 'home'}
      <Home />
    {:else}
      <Playlists />
    {/if}
    </SheetArea>
    </div>
    {#if settingsSplit || insightsSplit}
      <div class="grip line" aria-hidden="true"></div>
    {:else if sidebar}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div
        class="grip edge-grip"
        role="separator"
        aria-orientation="vertical"
        aria-label="Player width"
        aria-valuenow={playerWidth}
        aria-valuemin={PLAYER_MIN}
        aria-valuemax={area.w - PAGE_MIN}
        tabindex="0"
        onpointerdown={grip}
        onkeydown={gripKey}
      ></div>
    {/if}
    {#if sidebar}
      <!-- The mini-player floats over the panel's end too: its lists leave room for it (--mini-h). -->
      <aside class="player-side" class:whole={settingsSplit || insightsSplit} aria-label={panelShowsPlayer() ? 'Player' : settingsShown ? 'Settings' : insightsShown ? 'Insights' : 'Playlist'} style:--mini-h={`${miniSpace}px`} bind:this={sideEl}>
        <SheetArea area={() => sideEl}>
          {#if visualsBeside}<Visuals docked onclose={() => (visuals.open = false)} />{:else if panelShowsPlayer()}<NowPlaying place="side" />{:else if settingsShown}<SettingsPanel section={settingsShown} beside={settingsSplit} />{:else if insightsShown}<InsightsPanel />{:else}<PlaylistView />{/if}
        </SheetArea>
      </aside>
    {/if}
  </div>

  {#if playlistLayer}
    <div class="playlist-layer" role="dialog" aria-label="Playlist" style:--mini-h={`${miniSpace}px`} transition:fade={{ duration: reducedMotion() ? 0 : 200 }}>
      {#if settingsShown}<SettingsPanel section={settingsShown} />{:else if insightsShown}<InsightsPanel />{:else}<PlaylistView />{/if}
    </div>
  {/if}
  {#if mini}
    <div class="mini-dock" class:over={playlistLayer}><MiniPlayer /></div>
  {/if}
  <!-- Played on from elsewhere before this page was touched: the browser holds the sound back until a tap. -->
  {#if target.tapToPlay}
    <button
      class="tap-to-play"
      onclick={() => {
        target.tapToPlay = false;
        if (listening.leader) resumeGroup();
        else togglePlay();
      }}><Icon name="play" size={20} />Tap to play</button
    >
  {/if}
  {#if pageRow}
    <div class="page-row" class:beside={sidebar} class:insights={page === 'insights'} class:open={page === 'insights' && panel.viewed !== null} style:--player-w="{sideWidth}px">
      {#if navAt > 0}
        <span class="navpill"><button aria-label="Back" title="Back" onclick={() => history.back()}><Icon name="back" size={22} /></button></span>
      {:else}
        <span></span>
      {/if}
      {#if pagePill}<SheetArea area={() => pageEl}><TopPill {page} view={hasView(page)} device={pillDevice} filter={!page.startsWith('local-')} /></SheetArea>{/if}
      {#if page === 'insights'}
        <InsightsTabs />
        <!-- Its panel's button while no panel is open (the panel closes itself, top left). -->
        {#if panel.viewed === null}<span class="navpill"><button aria-label="View" title="View" onclick={() => viewPlaylist(INSIGHTS_VIEW)}><Icon name="sliders" size={22} /></button></span>{/if}
      {/if}
      {#if page === 'settings'}
        <span class="navpill"><button aria-label="About" title="About" onclick={() => (aboutOpen = true)}><Icon name="info" size={22} /></button></span>
      {/if}
    </div>
  {/if}
  {#if overlay}
    <!-- Over everything, as Finamp's: it slides up when it opens (300 ms, easeInOutQuad) and fades when it closes
         (300 ms, linear). -->
    <div class="overlay" role="dialog" aria-label="Player" in:fly={{ y: innerHeight, duration: reducedMotion() ? 0 : 300, easing: quadInOut }} out:fade={{ duration: reducedMotion() ? 0 : 300, easing: linear }}>
      <NowPlaying place="overlay" />
    </div>
  {/if}

  {#if aboutOpen}<AboutSheet onclose={() => (aboutOpen = false)} />{/if}

  {#if moreOpen}
    <button class="scrim" aria-label="Close menu" onclick={() => (moreOpen = false)}></button>
    <div class="sheet" id="more-sheet" role="dialog" aria-label="More pages" onclickcapture={collapse}>
      {#each overflow as p (p.id)}
        <a href={keepViewed(p.href)} aria-current={page === p.id ? 'page' : undefined}><Icon name={p.icon} />{p.label}{#if p.id === 'library' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</a>
      {/each}
    </div>
  {/if}

  {#if visuals.open && !visualsBeside}
    <Visuals onclose={() => (visuals.open = false)} />
  {/if}
  <Toasts />
<LabelTip />
  <ProgressBar />

  <nav class="bar" aria-label="Pages" onclickcapture={collapse}>
    <!-- The mode switcher's slim row over the tabs (decision 0052, mock C), with a local folder on. -->
    {#if switchable()}<div class="slim"><ModeSwitcher slim {connection} /></div>{/if}
    <div class="tabs">
    {#each bar as p (p.id)}
      <a href={keepViewed(p.href)} aria-current={page === p.id ? 'page' : undefined}
        ><span class="icon"><Icon name={p.icon} />{#if p.id === 'library' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</span><span>{p.label}</span></a
      >
    {/each}
    {#if overflow.length}
      <button
        class:active={overflowActive}
        aria-expanded={moreOpen}
        aria-controls="more-sheet"
        onclick={() => (moreOpen = !moreOpen)}><Icon name="more" /><span>More</span></button
      >
    {/if}
    </div>
  </nav>
</div>

<style>
  /* Phone first: content full width, navigation in a bar at the bottom. From 1024 px (as seerr does) a fixed
     sidebar takes over and the bar goes away. */
  .nav-col {
    display: none;
  }

  /* Below 1024 px the bar of pages is fixed along the top; the mini-player floats along the bottom (decision 0018).
     The page keeps clear of both (--top-h, --bottom-h, set by the script). The room under it belongs to the page
     column only: under the content as a whole it would add to the full-height panel beside the page and make the
     window scroll. */
  .content {
    min-width: 0;
    padding-top: calc(var(--top-h, 0px) + env(safe-area-inset-top));
  }

  /* Layers, bottom to top (decision 0023): the page, its navigation (20), a viewed playlist (21), the mini-player
     (22), the player over everything (30). */
  .tap-to-play {
    position: fixed;
    left: 50%;
    bottom: calc(112px + env(safe-area-inset-bottom));
    transform: translateX(-50%);
    z-index: 30;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 12px 20px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }

  .mini-dock {
    position: fixed;
    inset: auto 12px calc(12px + env(safe-area-inset-bottom));
    z-index: 22;
  }

  /* A viewed playlist without room beside the page (decision 0023): full height over the page and its navigation, the
     mini-player floating over its end (its list leaves room for it, --mini-h), the player over everything. */
  .playlist-layer {
    position: fixed;
    inset: 0;
    z-index: 21;
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    background: var(--surface);
  }

  /* The page's row: back and the pill floating over the page's top, which starts under them (.under-row); under the page
     bar on a phone, beside the navigation and left of the panel on a wide window. Over the page and its bar, under a
     viewed playlist. */
  .page-row {
    position: fixed;
    top: calc(var(--top-h, 0px) + env(safe-area-inset-top) + 8px);
    left: 12px;
    right: 12px;
    z-index: 20;
    display: flex;
    justify-content: space-between;
    /* Back stays round when what is beside it grows a row (Insights' pill). */
    align-items: flex-start;
    /* Never touching: back and what is beside it (the owner, 2026-10-23). */
    gap: 8px;
    pointer-events: none;
  }

  .page-row > :global(*) {
    pointer-events: auto;
  }

  .page-row.beside {
    right: calc(var(--player-w) + 10px + 12px);
  }

  /* Insights: its tabs centred between back and its panel's button; with a panel open, at the row's end (the owner,
     2026-10-23). Back's and the button's columns are as wide either way. */
  .page-row.insights {
    display: grid;
    grid-template-columns: 52px minmax(0, 460px) 52px;
  }

  .page-row.insights.open {
    grid-template-columns: 52px minmax(0, 460px);
  }

  .page.under-row {
    padding-top: 60px;
  }

  /* Back, round as the pill (lib/TopPill). */
  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 22px;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .overlay {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }

  /* The player beside the page (decision 0018): page | grip | player, the player as tall as the window above the
     bottom bar and staying in place while the page scrolls. */
  .content.with-side {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 1px var(--player-w);
    align-items: start;
  }

  .page {
    min-width: 0;
    padding-bottom: calc(var(--bottom-h, 0px) + env(safe-area-inset-bottom));
  }

  .grip,
  .player-side {
    position: sticky;
    top: calc(var(--top-h, 0px) + env(safe-area-inset-top));
    height: calc(var(--app-h, 100dvh) - var(--top-h, 0px) - env(safe-area-inset-top));
  }

  /* The edge between page and player wears the shared handle (app.css .edge-grip). Beside Settings' list it is a plain
     line: no handle, so no cut in the panel for one (.whole). */
  .grip.line {
    width: 1px;
    background: var(--border);
  }

  .player-side.whole {
    mask: none;
  }

  /* The panel scrolls on its own: a scroll in it never moves the page beside it (nor where it has nothing to scroll).
     Its content answers to the panel's width (a container), which can be dragged narrow. */
  .player-side {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    overscroll-behavior: contain;
    container-type: inline-size;
    background: var(--bg);
  }

  /* Where the edge's handle reaches over the panel (app.css .edge-grip, centred on the line just left of it), the panel
     is cut away around it with a pixel clear all round, so the handle never sits on its art. The cut's shape is the
     handle's grown by that pixel (its size here, 7 by 46, follows --grip-w and --grip-h). */
  .player-side {
    --notch-w: calc(var(--grip-w) + 2px);
    --notch-h: calc(var(--grip-h) + 2px);
    mask:
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 7 46' preserveAspectRatio='none'%3E%3Crect width='7' height='46' rx='3.5'/%3E%3C/svg%3E")
        calc(-0.5px - var(--notch-w) / 2) 50% / var(--notch-w) var(--notch-h) no-repeat,
      linear-gradient(#000 0 0);
    mask-composite: exclude;
  }

  /* Same place and look as a page's own empty state. */
  .no-music {
    max-width: 900px;
    margin: 16px 0;
    padding: 0 clamp(12px, 2vw, 32px);
    color: var(--muted);
    font-size: 0.9rem;
  }

  .bar {
    position: fixed;
    inset: 0 0 auto;
    z-index: 20;
    display: flex;
    flex-direction: column;
    padding: calc(6px + env(safe-area-inset-top)) 8px 6px;
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    backdrop-filter: blur(12px);
    border-bottom: 1px solid var(--border);
  }

  .bar .tabs {
    display: flex;
    justify-content: space-around;
  }

  .bar .slim {
    padding: 0 4px 4px;
  }

  .bar a,
  .bar .tabs > button {
    flex: 1 1 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    min-height: 52px;
    max-width: 120px;
    padding: 6px 4px;
    background: none;
    border: 0;
    border-radius: 12px;
    color: var(--muted);
    font-size: 0.72rem;
    text-decoration: none;
    cursor: pointer;
  }

  .bar a[aria-current='page'],
  .bar button.active {
    color: var(--accent);
    font-weight: 600;
  }

  /* How many albums wait in the conversion queue, on the Incoming tab. */
  .count {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    box-sizing: border-box;
    border-radius: 9px;
    background: var(--accent);
    color: var(--accent-text);
    font-size: 0.7rem;
    font-weight: 600;
    line-height: 18px;
    text-align: center;
  }

  .sheet .count,
  .sidebar .count {
    margin-left: auto;
  }

  .bar .icon {
    position: relative;
    display: flex;
  }

  .bar .count {
    position: absolute;
    top: -6px;
    left: calc(100% - 6px);
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 15;
    padding: 0;
    background: rgb(0 0 0 / 0.45);
    border: 0;
  }

  .sheet {
    position: fixed;
    inset: calc(var(--top-h, 0px) + env(safe-area-inset-top) + 8px) 8px auto;
    z-index: 21;
    display: grid;
    gap: 2px;
    padding: 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.35);
  }

  .sheet a,
  .sidebar nav a,
  .foot a {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0 12px;
    border-radius: 10px;
    color: var(--text);
    text-decoration: none;
  }

  .sheet a:hover,
  .sidebar nav a:hover,
  .foot a:hover {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  .sheet a[aria-current='page'],
  .sidebar nav a[aria-current='page'],
  .foot a[aria-current='page'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  @media (min-width: 1024px) {
    .shell {
      --nav-w: 240px;
      display: grid;
      grid-template-columns: var(--nav-w) minmax(0, 1fr);
    }

    /* Its own column always, also while the navigation shows over it (out of the grid's flow). */
    .content {
      grid-column: 2;
    }

    /* Over the page beside it: its handle reaches over the page's edge. */
    .nav-col {
      position: sticky;
      top: 0;
      z-index: 10;
      height: var(--app-h, 100dvh);
      display: flex;
    }

    .sidebar {
      flex: 1;
      min-width: 0;
      display: flex;
      flex-direction: column;
      padding: 20px 14px calc(14px + env(safe-area-inset-bottom));
      box-sizing: border-box;
      /* Never cut off: on a window too low for all of it, it scrolls. */
      overflow-y: auto;
      background: var(--surface);
    }

    .grip,
    .player-side {
      top: 0;
      height: var(--app-h, 100dvh);
    }

    .mini-dock,
    .page-row {
      left: calc(var(--nav-w) + 12px);
    }

    /* A playlist over the page covers the navigation too: the mini-player floats over it with the same space on both
       sides, never pushed to one side by a navigation it is not beside (the owner, 2026-10-17). */
    .mini-dock.over {
      left: 12px;
    }

    .bar,
    .scrim,
    .sheet {
      display: none;
    }
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px;
  }

  /* The mark (lib/Mark): the record. */
  .logo {
    display: grid;
    width: 34px;
    height: 34px;
  }

  .brand .name {
    font-size: 1.35rem;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .conn {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 0 18px;
    padding: 0 8px;
    color: var(--muted);
    font-size: 0.82rem;
    text-decoration: none;
  }

  .conn i {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
  }

  .conn.down {
    color: var(--danger);
  }

  .conn.down i {
    background: var(--danger);
  }

  .sidebar nav {
    display: grid;
    gap: 2px;
  }

  .foot {
    display: grid;
    gap: 6px;
    margin-top: auto;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .version {
    padding: 0 12px;
    color: var(--muted);
    font-size: 0.75rem;
  }

  /* The navigation's edge: the panel's handle (app.css .edge-grip) as its right border, a tap or a drag. */
  .nav-grip {
    display: none;
  }

  /* Folded (decision 0042): a strip of icons; the names stay for screen readers. */
  @media (min-width: 1024px) {
    .shell.folded {
      --nav-w: 68px;
    }

    .folded .sidebar:not(.peek) {
      padding-inline: 10px;
    }

    .folded .sidebar:not(.peek) :is(nav a, .foot a, .conn, .brand) {
      justify-content: center;
      padding-inline: 0;
    }

    .folded .sidebar:not(.peek) :is(.label, .name) {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }

    .folded .sidebar:not(.peek) .version {
      display: none;
    }

    .folded .sidebar:not(.peek) nav a {
      position: relative;
    }

    .folded .sidebar:not(.peek) .count {
      position: absolute;
      top: 2px;
      right: 4px;
      margin: 0;
    }

    .nav-grip {
      display: block;
      position: absolute;
      top: 0;
      right: 0;
      bottom: 0;
      padding: 0;
      border: 0;
    }

    /* Shown whole over the page: the page keeps its place beside the strip. */
    .nav-col.peek {
      position: fixed;
      left: 0;
      top: 0;
      z-index: 25;
      width: 240px;
      box-shadow: 8px 0 32px rgb(0 0 0 / 0.45);
    }
  }
</style>
