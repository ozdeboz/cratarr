<script lang="ts">
  import Playlists from './Playlists.svelte';
  import Health from './Health.svelte';
  import Settings from './Settings.svelte';
  import Library from './Library.svelte';
  import Insights from './Insights.svelte';
  import Downloads from './Downloads.svelte';
  import SettingsPanel from './lib/SettingsPanel.svelte';
  import { SECTIONS, type SettingsSection } from './lib/settingsSections.svelte';
  import { askDownloads, downloads, inApp } from './lib/app.svelte';
  import { reach } from './lib/reach.svelte';
  import { keepCoversAway } from './lib/coversAway.svelte';
  import Crates from './Crates.svelte';
  import AboutSheet from './lib/AboutSheet.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import { target } from './lib/playOnTarget.svelte';
  import { togglePlay } from './lib/localPlayer.svelte';
  import NowPlaying from './lib/NowPlaying.svelte';
  import SheetArea from './lib/SheetArea.svelte';
  import MiniPlayer from './lib/MiniPlayer.svelte';
  import TopPill from './lib/TopPill.svelte';
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
  import { closePlaylist, keepViewed, panel, settingsOf } from './lib/panel.svelte';
  import PlaylistView from './lib/PlaylistView.svelte';
  import { poll } from './lib/live.svelte';
  import { known, remember } from './lib/sources.svelte';
  import { getStatus, type Status } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { jobs } from './lib/jobs.svelte';

  type Page = { id: string; href: string; label: string; icon: IconName };
  // Main pages in order; Insights, Health and Settings sit apart at the bottom of the sidebar, in that order (decision
  // 0025: Health beside Settings, Insights beside Health).
  const PLAYLISTS: Page = { id: 'playlists', href: '#/', label: 'Playlists', icon: 'playlists' };
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
    return [HEALTH, SETTINGS, LIBRARY, INSIGHTS, CRATES, DOWNLOADS].find((p) => p.href === h)?.id ?? PLAYLISTS.id;
  };
  let page = $state(fromHash());
  // Where the player shows (decision 0018): beside the page on a roomy window while something plays (`sidebar`);
  // otherwise the mini-player along the bottom (`mini`), which opens it over everything (`overlay`). The player over
  // everything closes when the page changes or the navigation is used, and the page behind it does not scroll.
  const collapse = () => (player.full = false);
  $effect(() => {
    page;
    untrack(collapse);
  });
  const sidebar = $derived(inSidebar());
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
  /** The mini-player's height with its margin, kept free under the page and the toasts. */
  const MINI_SPACE = 88;
  const miniSpace = $derived(mini && playing() ? MINI_SPACE : 0);
  // The page's row (Symfonium's, decided 2026-10-15), floating over the page's top: back on the left, when this visit
  // has a step before this one; on the right, on a page with something to play, the pill (lib/TopPill): the filter
  // always, the page's View, and where it plays when nothing else leads there (no mini-player, no player beside the
  // page). The page starts under the row, so at rest nothing lies under it.
  const PLAYABLE = ['playlists', 'crates', 'library', 'downloads'];
  // A part of the settings in the panel (lib/SettingsPanel), when the panel shows one.
  const settingsShown = $derived(SECTIONS.find((x) => x.id === settingsOf(panel.viewed))?.id ?? (null as SettingsSection | null));
  const pagePill = $derived(PLAYABLE.includes(page));
  // An icon button's label on a long-press or right-click, everywhere (lib/labels).
  $effect(installLabels);
  // Each page starts with its search empty (the pill's field).
  $effect(() => {
    void page;
    pageView.query = '';
  });
  // The page's device button gives way to the panel's (the owner, 2026-10-16): a list beside the page has its own.
  const panelDevice = $derived(sidebar && panel.viewed !== null && !panelShowsPlayer() && !settingsShown);
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
  const pageRow = $derived(!playlistLayer && !overlay && (phone || navAt > 0 || pagePill || page === 'settings'));
  // The settings page's info button (the owner, 2026-10-16): About in a sheet.
  let aboutOpen = $state(false);
  $effect(() => {
    const root = document.documentElement.style;
    root.setProperty('--top-h', phone ? `${TOP_BAR}px` : '0px');
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
    const on = () => (area = { w: innerWidth - (phone ? 0 : NAV_WIDTH), h: innerHeight - (phone ? TOP_BAR : 0) });
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
    if (downloads.tracks.length && page === PLAYLISTS.id && !location.hash.replace(/^#\/?/, '')) {
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

  // The conversion queue, followed on every page: its count on the Incoming tab, a toast when an album is done.
  $effect(() => jobs.start());
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
  const MAIN: Page[] = [LIBRARY, CRATES, PLAYLISTS];
  // Apart, at the bottom of the sidebar: Insights beside Health, then Settings.
  const APART: Page[] = inApp ? [DOWNLOADS, INSIGHTS, HEALTH, SETTINGS] : [INSIGHTS, HEALTH, SETTINGS];
  const ALL = $derived([...MAIN, ...APART]);
  const bar = $derived(ALL.length <= BAR_SLOTS ? ALL : ALL.slice(0, BAR_SLOTS - 1));
  const overflow = $derived(ALL.filter((p) => !bar.includes(p)));
  const overflowActive = $derived(overflow.some((p) => p.id === page));
</script>

<div class="shell">
  <aside class="sidebar" onclickcapture={collapse}>
    <div class="brand">
      <span class="logo"><Mark /></span>
      <span class="name">Cratarr</span>
    </div>
    {#if connection}
      <a class="conn" class:down={!connection.ok} href={keepViewed("#/settings")}><i aria-hidden="true"></i>{connection.text}</a>
    {/if}

    <nav aria-label="Pages">
      {#each MAIN as p (p.id)}
        <a href={keepViewed(p.href)} aria-current={page === p.id ? 'page' : undefined}><Icon name={p.icon} />{p.label}{#if p.id === 'library' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</a>
      {/each}
    </nav>

    <div class="foot">
      {#each APART as p (p.id)}
        <a href={keepViewed(p.href)} aria-current={page === p.id ? 'page' : undefined}><Icon name={p.icon} />{p.label}</a>
      {/each}
      {#if status}<div class="version">Cratarr {status.version}</div>{/if}
    </div>
  </aside>

  <div class="content" class:with-side={sidebar} style:--player-w="{playerWidth}px" bind:this={contentEl}>
    <div class="page" class:under-row={pageRow} bind:this={pageEl}>
    <SheetArea area={() => pageEl}>
    {#if page === 'settings'}
      <Settings />
    {:else if page === 'health'}
      <Health />
    {:else if page === 'downloads' && inApp}
      <Downloads />
    {:else if page === 'library'}
      <Library />
    {:else if status && !status.music}
      <p class="no-music">No Music library is set. Choose one in Settings → Libraries.</p>
    {:else if page === 'insights'}
      <Insights />
    {:else if page === 'crates'}
      <Crates />
    {:else}
      <Playlists />
    {/if}
    </SheetArea>
    </div>
    {#if sidebar}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div
        class="grip"
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
      <!-- The mini-player floats over the panel's end too: its lists leave room for it (--mini-h). -->
      <aside class="player-side" aria-label={panelShowsPlayer() ? 'Player' : 'Playlist'} style:--mini-h={`${miniSpace}px`} bind:this={sideEl}>
        <SheetArea area={() => sideEl}>
          {#if panelShowsPlayer()}<NowPlaying place="side" />{:else if settingsShown}<SettingsPanel section={settingsShown} />{:else}<PlaylistView />{/if}
        </SheetArea>
      </aside>
    {/if}
  </div>

  {#if playlistLayer}
    <div class="playlist-layer" role="dialog" aria-label="Playlist" style:--mini-h={`${miniSpace}px`} transition:fade={{ duration: reducedMotion() ? 0 : 200 }}>
      {#if settingsShown}<SettingsPanel section={settingsShown} />{:else}<PlaylistView />{/if}
    </div>
  {/if}
  {#if mini}
    <div class="mini-dock"><MiniPlayer /></div>
  {/if}
  <!-- Played on from elsewhere before this page was touched: the browser holds the sound back until a tap. -->
  {#if target.tapToPlay}
    <button
      class="tap-to-play"
      onclick={() => {
        target.tapToPlay = false;
        togglePlay();
      }}><Icon name="play" size={20} />Tap to play</button
    >
  {/if}
  {#if pageRow}
    <div class="page-row" class:beside={sidebar} style:--player-w="{playerWidth}px">
      {#if navAt > 0}
        <span class="navpill"><button aria-label="Back" title="Back" onclick={() => history.back()}><Icon name="back" size={22} /></button></span>
      {:else}
        <span></span>
      {/if}
      {#if pagePill}<SheetArea area={() => pageEl}><TopPill {page} view={hasView(page)} device={pillDevice} /></SheetArea>{/if}
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

  <Toasts />
<LabelTip />
  <ProgressBar />

  <nav class="bar" aria-label="Pages" onclickcapture={collapse}>
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
  </nav>
</div>

<style>
  /* Phone first: content full width, navigation in a bar at the bottom. From 1024 px (as seerr does) a fixed
     sidebar takes over and the bar goes away. */
  .sidebar {
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
    pointer-events: none;
  }

  .page-row > :global(*) {
    pointer-events: auto;
  }

  .page-row.beside {
    right: calc(var(--player-w) + 10px + 12px);
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
    grid-template-columns: minmax(0, 1fr) 10px var(--player-w);
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

  /* The edge between page and player: a line, with a handle in the middle that shows it can be dragged. */
  .grip {
    display: grid;
    place-items: center;
    cursor: col-resize;
    touch-action: none;
    background: linear-gradient(var(--border), var(--border)) center / 1px 100% no-repeat;
  }

  .grip::after {
    content: '';
    width: 6px;
    height: 44px;
    border-radius: 3px;
    background: var(--muted);
    opacity: 0.6;
  }

  .grip:hover::after,
  .grip:focus-visible::after {
    background: var(--accent);
    opacity: 1;
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
    justify-content: space-around;
    padding: calc(6px + env(safe-area-inset-top)) 8px 6px;
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    backdrop-filter: blur(12px);
    border-bottom: 1px solid var(--border);
  }

  .bar a,
  .bar button {
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
      display: grid;
      grid-template-columns: 240px minmax(0, 1fr);
    }

    .sidebar {
      position: sticky;
      top: 0;
      height: var(--app-h, 100dvh);
      display: flex;
      flex-direction: column;
      padding: 20px 14px calc(14px + env(safe-area-inset-bottom));
      box-sizing: border-box;
      /* Never cut off: on a window too low for all of it, it scrolls. */
      overflow-y: auto;
      background: var(--surface);
      border-right: 1px solid var(--border);
    }

    .grip,
    .player-side {
      top: 0;
      height: var(--app-h, 100dvh);
    }

    .mini-dock,
    .page-row {
      left: calc(240px + 12px);
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
</style>
