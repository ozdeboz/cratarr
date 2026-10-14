<script lang="ts">
  import Playlists from './Playlists.svelte';
  import Health from './Health.svelte';
  import Settings from './Settings.svelte';
  import Incoming from './Incoming.svelte';
  import Insights from './Insights.svelte';
  import Crates from './Crates.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import SourcePicker from './lib/SourcePicker.svelte';
  import NowPlaying from './lib/NowPlaying.svelte';
  import MiniPlayer from './lib/MiniPlayer.svelte';
  import { playing } from './lib/remote.svelte';
  import { fade, fly } from 'svelte/transition';
  import { linear, quadInOut } from 'svelte/easing';
  import { reducedMotion } from './lib/motion';
  import Toasts from './lib/Toasts.svelte';
  import { toast } from './lib/toast.svelte';
  import ProgressBar from './lib/ProgressBar.svelte';
  import { untrack } from 'svelte';
  import { inSidebar, live, panelShowsPlayer, player } from './lib/live.svelte';
  import { closePlaylist, keepOpen, panel } from './lib/panel.svelte';
  import PlaylistView from './lib/PlaylistView.svelte';
  import CrateBuilder from './lib/CrateBuilder.svelte';
  import { builderOf } from './lib/crates.svelte';
  import { poll } from './lib/live.svelte';
  import { known } from './lib/sources.svelte';
  import { getStatus, type Status } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { jobs } from './lib/jobs.svelte';

  type Page = { id: string; href: string; label: string; icon: IconName };
  // Main pages in order; Insights, Health and Settings sit apart at the bottom of the sidebar, in that order (decision
  // 0025: Health beside Settings, Insights beside Health).
  const PLAYLISTS: Page = { id: 'playlists', href: '#/', label: 'Playlists', icon: 'playlists' };
  // Shown only when some source has an incoming library (see the status call below).
  const INCOMING: Page = { id: 'incoming', href: '#/incoming', label: 'Incoming', icon: 'incoming' };
  const INSIGHTS: Page = { id: 'insights', href: '#/insights', label: 'Insights', icon: 'insights' };
  // The second step after Incoming (decision 0024).
  const CRATES: Page = { id: 'crates', href: '#/crates', label: 'Crates', icon: 'crates' };
  const HEALTH: Page = { id: 'health', href: '#/health', label: 'Health', icon: 'health' };
  const SETTINGS: Page = { id: 'settings', href: '#/settings', label: 'Settings', icon: 'settings' };

  // Phones get a bottom bar. Up to five fit; beyond that the first four stay and the rest go under "More".
  const BAR_SLOTS = 5;

  // Playlists are created, renamed, deleted and repaired on the Playlists page and its panel now; old Manage and
  // Repairs links land there (decision 0025). The page is the address before `?` (what is opened rides along as
  // `?open=`, decision 0023).
  const fromHash = () => {
    const h = location.hash.split('?')[0];
    return h === HEALTH.href
      ? 'health'
      : h === SETTINGS.href
        ? 'settings'
        : h === INCOMING.href
          ? 'incoming'
          : h === INSIGHTS.href
            ? 'insights'
            : h === CRATES.href
              ? 'crates'
              : 'playlists';
  };
  let page = $state(fromHash());
  // The server picker belongs to the pages that act on what a server plays: Playlists (add it) and Incoming (play an
  // album). The player itself is part of the shell on every page (decision 0018, amended: Cratarr plays music itself).
  const withPicker = $derived(page === 'playlists' || page === 'incoming');
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
  // The mini-player stands in for the player wherever the panel does not show it (decision 0023).
  const mini = $derived(!!playing() && !(sidebar && panelShowsPlayer()));
  // An opened playlist without room beside the page: a layer of its own, under the mini-player and the player.
  const playlistLayer = $derived(!sidebar && panel.open !== null);
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
  $effect(() => {
    const root = document.documentElement.style;
    root.setProperty('--top-h', phone ? `${TOP_BAR}px` : '0px');
    root.setProperty('--bottom-h', mini ? `${MINI_SPACE}px` : '0px');
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
  const MAIN = $derived<Page[]>(status?.incoming ? [INCOMING, CRATES, PLAYLISTS] : [CRATES, PLAYLISTS]);
  // Apart, at the bottom of the sidebar: Insights beside Health, then Settings.
  const APART: Page[] = [INSIGHTS, HEALTH, SETTINGS];
  const ALL = $derived([...MAIN, ...APART]);
  const bar = $derived(ALL.length <= BAR_SLOTS ? ALL : ALL.slice(0, BAR_SLOTS - 1));
  const overflow = $derived(ALL.filter((p) => !bar.includes(p)));
  const overflowActive = $derived(overflow.some((p) => p.id === page));
</script>

<div class="shell">
  <aside class="sidebar" onclickcapture={collapse}>
    <div class="brand">
      <span class="logo"><Icon name="playlists" size={20} /></span>
      <span class="name">Cratarr</span>
    </div>
    {#if connection}
      <a class="conn" class:down={!connection.ok} href={keepOpen("#/settings")}><i aria-hidden="true"></i>{connection.text}</a>
    {/if}
    {#if !phone}
      <div class="side-picker" class:shown={known.list.length > 1}><SourcePicker /></div>
    {/if}

    <nav aria-label="Pages">
      {#each MAIN as p (p.id)}
        <a href={keepOpen(p.href)} aria-current={page === p.id ? 'page' : undefined}><Icon name={p.icon} />{p.label}{#if p.id === 'incoming' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</a>
      {/each}
    </nav>

    <div class="foot">
      {#each APART as p (p.id)}
        <a href={keepOpen(p.href)} aria-current={page === p.id ? 'page' : undefined}><Icon name={p.icon} />{p.label}</a>
      {/each}
      {#if status}<div class="version">Cratarr {status.version}</div>{/if}
    </div>
  </aside>

  <div class="content" class:with-side={sidebar} style:--player-w="{playerWidth}px" bind:this={contentEl}>
    <div class="page">
    {#if withPicker}
      {#if phone}<div class="top-picker" class:shown={known.list.length > 1}><SourcePicker /></div>{/if}
    {/if}
    {#if page === 'settings'}
      <Settings />
    {:else if page === 'health'}
      <Health />
    {:else if page === 'incoming' && status?.incoming}
      <Incoming />
    {:else if status && !status.music}
      <p class="no-music">No Music library is set. Choose one in Settings → Libraries.</p>
    {:else if page === 'insights'}
      <Insights />
    {:else if page === 'crates'}
      <Crates />
    {:else}
      <Playlists />
    {/if}
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
      <aside class="player-side" aria-label={panelShowsPlayer() ? 'Player' : 'Playlist'} style:--mini-h={mini ? `${MINI_SPACE}px` : '0px'}>
        {#if panelShowsPlayer()}<NowPlaying place="side" />{:else if builderOf(panel.open)}<CrateBuilder id={builderOf(panel.open)!.id} />{:else}<PlaylistView />{/if}
      </aside>
    {/if}
  </div>

  {#if playlistLayer}
    <div class="playlist-layer" role="dialog" aria-label="Playlist" style:--mini-h={mini ? `${MINI_SPACE}px` : '0px'} transition:fade={{ duration: reducedMotion() ? 0 : 200 }}>
      {#if builderOf(panel.open)}<CrateBuilder id={builderOf(panel.open)!.id} />{:else}<PlaylistView />{/if}
    </div>
  {/if}
  {#if mini}
    <div class="mini-dock"><MiniPlayer /></div>
  {/if}
  {#if overlay}
    <!-- Over everything, as Finamp's: it slides up when it opens (300 ms, easeInOutQuad) and fades when it closes
         (300 ms, linear). -->
    <div class="overlay" role="dialog" aria-label="Player" in:fly={{ y: innerHeight, duration: reducedMotion() ? 0 : 300, easing: quadInOut }} out:fade={{ duration: reducedMotion() ? 0 : 300, easing: linear }}>
      <NowPlaying place="overlay" />
    </div>
  {/if}

  {#if moreOpen}
    <button class="scrim" aria-label="Close menu" onclick={() => (moreOpen = false)}></button>
    <div class="sheet" id="more-sheet" role="dialog" aria-label="More pages" onclickcapture={collapse}>
      {#each overflow as p (p.id)}
        <a href={keepOpen(p.href)} aria-current={page === p.id ? 'page' : undefined}><Icon name={p.icon} />{p.label}{#if p.id === 'incoming' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</a>
      {/each}
    </div>
  {/if}

  <Toasts />
  <ProgressBar />

  <nav class="bar" aria-label="Pages" onclickcapture={collapse}>
    {#each bar as p (p.id)}
      <a href={keepOpen(p.href)} aria-current={page === p.id ? 'page' : undefined}
        ><span class="icon"><Icon name={p.icon} />{#if p.id === 'incoming' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</span><span>{p.label}</span></a
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

  /* Layers, bottom to top (decision 0023): the page, its navigation (20), an opened playlist (21), the mini-player
     (22), the player over everything (30). */
  .mini-dock {
    position: fixed;
    inset: auto 12px calc(12px + env(safe-area-inset-bottom));
    z-index: 22;
  }

  /* An opened playlist without room beside the page (decision 0023): full height over the page and its navigation, the
     mini-player floating over its end (its list leaves room for it, --mini-h), the player over everything. */
  .playlist-layer {
    position: fixed;
    inset: 0;
    z-index: 21;
    display: flex;
    flex-direction: column;
    background: var(--surface);
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
    height: calc(100dvh - var(--top-h, 0px) - env(safe-area-inset-top));
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

  /* The panel scrolls on its own: a scroll in it never moves the page beside it (nor where it has nothing to scroll). */
  .player-side {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    overscroll-behavior: contain;
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
      height: 100vh;
      height: 100dvh;
      display: flex;
      flex-direction: column;
      padding: 20px 14px 14px;
      background: var(--surface);
      border-right: 1px solid var(--border);
    }

    .grip,
    .player-side {
      top: 0;
      height: 100dvh;
    }

    .mini-dock {
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

  .logo {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-text);
  }

  .brand .name {
    font-size: 1.35rem;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  /* One picker at a time: the sidebar's from 1024 px, otherwise a row above the page. It takes no room until there
     is more than one source (the picker itself renders nothing then, but stays mounted to learn the sources). */
  .side-picker.shown {
    margin: -8px 8px 16px;
  }

  .top-picker.shown {
    padding: 10px clamp(12px, 2vw, 32px) 0;
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
