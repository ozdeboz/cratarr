<script lang="ts">
  import { installed } from './lib/offline.svelte';
  import { backdropUrl, nextBackdrop, SHUFFLE_EVERY_MS, shuffleStale, shuffleTurns } from './lib/backdrop.svelte';
  import Playlists from './Playlists.svelte';
  import LinkSheet from './lib/LinkSheet.svelte';
  import Discover from './Discover.svelte';
  import Health from './Health.svelte';
  import Settings from './Settings.svelte';
  import Library from './Library.svelte';
  import Home from './Home.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import LocalAlbums from './LocalAlbums.svelte';
  import ModeSwitcher from './lib/ModeSwitcher.svelte';
  import { localMode, switchable } from './lib/deviceMusic.svelte';
  import { serverConnection } from './lib/connection.svelte';
  import Insights from './Insights.svelte';
  import Downloads from './Downloads.svelte';
  import Requests from './Requests.svelte';
  import SettingsPanel from './lib/SettingsPanel.svelte';
  import HealthPanel from './lib/HealthPanel.svelte';
  import { health, problems } from './lib/healthReport.svelte';
  import { SECTIONS, type SettingsSection } from './lib/settingsSections.svelte';
  import { askDownloads, downloads, inApp } from './lib/app.svelte';
  import './lib/keyboard.svelte';
  import './lib/described.svelte';
  import { kept } from './lib/kept.svelte';
  import { later } from './lib/later.svelte';
  // What changed elsewhere while away, read again (decision 0087).
  import './lib/changes.svelte';
  import { reach } from './lib/reach.svelte';
  import { keepCoversAway } from './lib/coversAway.svelte';
  import Visuals from './lib/visuals/Visuals.svelte';
  import { visuals } from './lib/visuals/shown.svelte';
  import Crates from './Crates.svelte';
  import AboutSheet, { about } from './lib/AboutSheet.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import { target } from './lib/playOnTarget.svelte';
  import { listening, resumeGroup } from './lib/groupListen.svelte';
  import { startWith, togglePlay } from './lib/localPlayer.svelte';
  import NowPlaying from './lib/NowPlaying.svelte';
  import SheetArea from './lib/SheetArea.svelte';
  import Hold from './lib/Hold.svelte';
  import { handing, HANDOVER_GIVE_UP_MS, nextHeld, NONE, type Places } from './lib/handover';
  import MiniPlayer from './lib/MiniPlayer.svelte';
  import TopPill from './lib/TopPill.svelte';
  import InsightsPanel from './lib/InsightsPanel.svelte';
  import { INSIGHTS_VIEWS, viewNow, type InsightsView } from './lib/insightsViews';
  import Mark from './lib/Mark.svelte';
  import LabelTip from './lib/LabelTip.svelte';
  import { installLabels } from './lib/labels.svelte';
  import { installKeyWind } from './lib/keyWind';
  import { hasView, pageView } from './lib/pageView.svelte';
  import { inSpotify, loadSpotify, spotify } from './lib/spotify.svelte';
  import { art, playing } from './lib/remote.svelte';
  import { fade, fly, slide } from 'svelte/transition';
  import { linear, quadInOut } from 'svelte/easing';
  import { decelerate, reducedMotion, SHEET_IN, SHEET_OUT } from './lib/motion';
  import { glass } from './lib/glass.svelte';
  import GlobalResults from './lib/GlobalResults.svelte';
  import RecordsPicker from './lib/RecordsPicker.svelte';
  import DiscoverPicker from './lib/DiscoverPicker.svelte';
  import { chooseKind, kindNow, recordKinds } from './lib/recordKinds';
  import { DISCOVER_MODES, SPOTIFY_VIEWS, type DiscoverMode } from './lib/discoverModes';
  import { globalSearch, pageKinds } from './lib/globalSearch.svelte';
  import { leaveOnArrival, leavePlayer, navPlace, onArrival } from './lib/navStep.svelte';
  import { provideEverywhere } from './lib/everywhere.svelte';
  import Find from './Find.svelte';
  import Toasts from './lib/Toasts.svelte';
  import ManyConfirm from './lib/ManyConfirm.svelte';
  import { toast } from './lib/toast.svelte';
  import ProgressBar from './lib/ProgressBar.svelte';
  import { untrack } from 'svelte';
  import { inSidebar, live, panelShowsPlayer, player } from './lib/live.svelte';
  import { followAddress } from './lib/panel.svelte';
  import { closePlaylist, HEALTH as HEALTH_VIEW, healthOf, INSIGHTS as INSIGHTS_VIEW, insightsOf, keepViewed, openedHere, panel, REQUESTS as REQUESTS_VIEW, requestsOf, DOWNLOADS as DOWNLOADS_VIEW, downloadsOf, SETTINGS as SETTINGS_VIEW, settingsOf, viewInPlace, viewPlaylist } from './lib/panel.svelte';
  import RequestsPanel from './lib/RequestsPanel.svelte';
  import DownloadsPanel from './lib/DownloadsPanel.svelte';
  import { DOWNLOADS_LISTS, type DownloadsList } from './lib/downloadsLists.svelte';
  import { answersRequests, REQUESTS_LISTS, requests as requestsData, type RequestsList } from './lib/requests.svelte';
  import PlaylistView from './lib/PlaylistView.svelte';
  import { poll } from './lib/live.svelte';
  import { known, remember } from './lib/sources.svelte';
  import { getStatus, type Status } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { jobs } from './lib/jobs.svelte';
  import { loadMe, me } from './lib/me.svelte';
  import { loadMine } from './lib/mySettings.svelte';
  // The Downloads shelf (decision 0085, step 2): it follows the downloads from the start, so Downloads works offline.
  import './lib/shelfStore.svelte';
  // The search's matcher (0.12): fetched once the page is idle, so it is on the device before the network may go; the
  // pill's lists read again when it is in.
  import { loadMatcherWhenIdle } from './lib/matcherLoad';
  import { matcherLoaded } from './lib/pageView.svelte';
  loadMatcherWhenIdle(matcherLoaded);

  type Page = { id: string; href: string; label: string; icon: IconName };
  // Main pages in order; Insights, Health and Settings sit apart at the bottom of the sidebar, in that order (decision
  // 0025: Health beside Settings, Insights beside Health).
  const PLAYLISTS: Page = { id: 'playlists', href: '#/', label: 'Playlists', icon: 'playlists' };
  // Albums outside the library, from what is listened to (decision 0069): beside Playlists (the owner, 2026-10-24).
  const DISCOVER: Page = { id: 'discover', href: '#/discover', label: 'Discover', icon: 'compass' };
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
  // Albums asked for from Discover, and their answers (decision 0070): left of Downloads (the owner, 2026-10-24), for
  // whoever may ask or answer.
  const REQUESTS: Page = { id: 'requests', href: '#/requests', label: 'Requests', icon: 'inbox' };
  // In the app only: what is kept on this device (decision 0033, step 4), playable without a connection.
  const DOWNLOADS: Page = { id: 'downloads', href: '#/downloads', label: 'Downloads', icon: 'download' };
  /** The pages that are an index with their lists in the panel (Requests, Downloads, Insights, Health, Settings): their
   *  name on the page beside back and their panel under the top bar, not over it, both wherever the bar of pages is
   *  along the top (`phone`: under 1024 px; the owner, 2026-10-25). */
  const PANEL_PAGES: Page[] = [REQUESTS, DOWNLOADS, INSIGHTS, HEALTH, SETTINGS];
  /** The kind a page already shows, left out of the global results under it (decision 0072). */
  const panelPage = $derived(PANEL_PAGES.find((p) => p.id === page) ?? null);
  // Local mode's (decision 0052): the device's own music, and the menu shows only these (and Settings). Its records
  // alone, as the Library's (the owner, 2026-10-29: one rule for both modes; its Artists and Tracks went).
  const LOCAL_ALBUMS: Page = { id: 'local-albums', href: '#/local/albums', label: 'Records', icon: 'disc' };
  const LOCAL = [LOCAL_ALBUMS];
  /** Find more music (0.9.3): the words looked up on MusicBrainz, reached from a search that found nothing anywhere. */
  const FIND: Page = { id: 'find', href: '#/find', label: 'Find more music', icon: 'search' };

  // Phones get a bottom bar. Up to five fit; beyond that the first four stay and the rest go under "More".
  const BAR_SLOTS = 5;
  /** A tab's least width in the bar: more fit on a wider screen. */
  const BAR_SLOT_PX = 72;

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
    // 0.9.16's addresses of the Library's pages (`#/library/tracks`): the Library, its one page now.
    if (h.startsWith(`${LIBRARY.href}/`)) return LIBRARY.id;
    return [HOME, HEALTH, SETTINGS, LIBRARY, INSIGHTS, CRATES, DISCOVER, REQUESTS, DOWNLOADS, FIND, ...LOCAL].find((p) => p.href === h)?.id ?? PLAYLISTS.id;
  };
  let page = $state(fromHash());
  // Where the player shows (decision 0018): beside the page on a roomy window while something plays (`sidebar`);
  // otherwise the mini-player along the bottom (`mini`), which opens it over everything (`overlay`). The player over
  // everything closes when the page changes or the navigation is used, and the page behind it does not scroll.
  // A link in it: the player closes once the page gone to shows (lib/navStep: it covered it meanwhile); anything else
  // there, at once.
  const collapse = (e?: MouseEvent) => {
    if ((e?.target as Element | null)?.closest?.('a[href]')) leavePlayer();
    else player.full = false;
  };
  onArrival((l) => l.player && (player.full = false));
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
  /** A tap on the link of the page that shows: no step in the history (a back button for nothing, the owner 2026-10-23),
   *  only the page back at its top. */
  function samePage(e: MouseEvent, id: string) {
    if (id !== page) return;
    e.preventDefault();
    pageEl?.scrollTo({ top: 0, behavior: 'smooth' });
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }
  /** The bar of pages (and its More sheet) on a phone: the panel over the page closed, so the page gone to shows (the
   *  owner, 2026-10-25); its own page's link closes the panel there, else takes the page to its top. */
  function barTap(e: MouseEvent, id: string) {
    if (playlistLayer && id === page) {
      e.preventDefault();
      closePlaylist();
      return;
    }
    samePage(e, id);
  }
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
  // Health as Settings (the owner, 2026-10-23): its problems' list beside one problem, the panel always beside it on a
  // roomy window.
  const healthSplit = $derived(page === 'health' && me.admin && player.roomy);
  const healthShown = $derived(healthOf(panel.viewed));
  // Insights as Settings and Health (the owner, 2026-10-24): its views' list beside one view, the panel always beside it
  // on a roomy window.
  const insightsSplit = $derived(page === 'insights' && player.roomy);
  const insightsShown = $derived(INSIGHTS_VIEWS.find((v) => v.id === insightsOf(panel.viewed))?.id ?? (null as InsightsView | null));
  // Downloads as Insights and Requests (the owner, 2026-10-24): its index beside the list opened, the panel always
  // beside it on a roomy window; its own panel, never the content panel.
  const downloadsSplit = $derived(page === 'downloads' && player.roomy);
  const downloadsShown = $derived(DOWNLOADS_LISTS.find((l) => l.id === downloadsOf(panel.viewed))?.id ?? (null as DownloadsList | null));
  // Requests as Insights (the owner, 2026-10-24): its index beside the list opened, the panel always beside it on a
  // roomy window.
  const requestsSplit = $derived(page === 'requests' && player.roomy);
  const requestsShown = $derived(REQUESTS_LISTS.find((l) => l.id === requestsOf(panel.viewed))?.id ?? (null as RequestsList | null));
  const split = $derived(settingsSplit || healthSplit || insightsSplit || downloadsSplit || requestsSplit);
  const sidebar = $derived(split || inSidebar());
  $effect(() => {
    panel.split = split;
  });
  // Over the page unless the player is beside it: a split page (Settings, Health, Insights) has the panel, not the
  // player, so with the player not wanted beside, the mini-player's tap opens it over the page there too (the owner,
  // 2026-10-24: in Settings › Display it would not open).
  const overlay = $derived(player.full && !inSidebar());
  // The player over everything has a step in the history while it is open (the owner, 2026-10-25: with the page out of
  // sight, back went back in the page unseen): back closes it, as it does the visuals. Closed another way (its button, a
  // swipe), its step goes with it.
  $effect(() => {
    if (!overlay) return;
    if (!history.state?.player) history.pushState({ ...(history.state ?? {}), player: true }, '');
    const popped = () => {
      if (!history.state?.player) player.full = false;
    };
    addEventListener('popstate', popped);
    return () => {
      removeEventListener('popstate', popped);
      if (history.state?.player) history.back();
    };
  });
  // A viewed playlist without room beside the page: a layer of its own, under the mini-player and the player.
  const playlistLayer = $derived(!sidebar && panel.viewed !== null);
  // One thing moving between its places (lib/handover; the owner, 2026-10-31: a panel made full screen closed first and
  // the page flashed): the place it leaves stays drawn until the place it goes to shows, then goes in the frame after.
  let held = $state<Places>(NONE);
  let placesWere: Places = untrack(() => ({ beside: sidebar, over: playlistLayer, overlay }));
  $effect.pre(() => {
    const now: Places = { beside: sidebar, over: playlistLayer, overlay };
    const was = placesWere;
    placesWere = now;
    untrack(() => (held = nextHeld(held, was, now)));
  });
  /** What came has shown: what it took over from goes (and after HANDOVER_GIVE_UP_MS whatever happens). */
  const letGoHeld = () => {
    if (handing(held)) held = NONE;
  };
  /** The player over everything, come by a hand-over: once its first frame is drawn. */
  const overlayShown = () => {
    requestAnimationFrame(() => requestAnimationFrame(() => overlay && letGoHeld()));
  };
  $effect(() => {
    if (!handing(held)) return;
    const t = setTimeout(letGoHeld, HANDOVER_GIVE_UP_MS);
    return () => clearTimeout(t);
  });
  const besideDrawn = $derived(sidebar || held.beside);
  const layerDrawn = $derived(playlistLayer || held.over);
  const overlayDrawn = $derived(overlay || held.overlay);
  /** Coming in by a hand-over: no entrance (it covers what it takes over from at once). */
  const handingNow = $derived(handing(held));
  // The visuals beside the page: in the panel where the player goes, while it is there (a narrow window: over the page).
  const visualsBeside = $derived(visuals.open && visuals.side && besideDrawn);
  // The mini-player stands in for the player wherever the panel does not show it (decision 0023); with nothing to play
  // it is the small button for where music plays.
  // The player in the panel: where nothing is viewed there, or brought over what is. On a page that keeps the panel
  // beside its list (Settings, Health, Insights, Downloads) only when brought, never in the moment between the page
  // showing and its part filling the panel (the owner, 2026-10-24: it flashed into the panel and back at each change).
  const playerInPanel = $derived(besideDrawn && (split ? panel.playerTop : panelShowsPlayer()));
  const mini = $derived(!playerInPanel);
  /** The layer as it shows (lib/Hold): opening, it stays unseen until what it holds is ready, and the page's glass and
   *  its room stay as they are until then (0.9.18; the owner, 2026-10-29: a crate opened full screen took the glass
   *  away before it showed, and the page jumped). Under it the page keeps both, covered; on a phone the row shows the
   *  panel's glass once the layer is seen. */
  const layerShown = $derived(playlistLayer && panel.shown !== null);
  $effect(() => {
    if (sidebar) untrack(collapse);
  });
  $effect(() => {
    document.documentElement.style.overflow = overlay || playlistLayer ? 'hidden' : '';
  });
  // Whose last copies the service worker keeps (public/sw.js): another account signing in, the last one's go.
  $effect(() => {
    if (me.name) navigator.serviceWorker?.controller?.postMessage({ type: 'account', name: me.name });
  });
  // One's own header picture (Settings › Appearance) behind every flow (the owner, 2026-10-26): one variable for the
  // page, each flow's canvas drawing it under a veil (--flow-photo; none without one). Only once it is in: until then
  // (and should it not come) the flows stand on the page's own ground, never a grey veil (the owner, 2026-10-27).
  // Shuffle (Settings › Appearance): another ticked one when the one there may no longer stand, and on its timer.
  $effect.pre(() => {
    if (shuffleStale()) nextBackdrop();
  });
  $effect(() => {
    if (!prefs.flowShuffle || prefs.flowShuffleWhen !== 'timed') return;
    shuffleTurns();
    const t = setTimeout(nextBackdrop, SHUFFLE_EVERY_MS);
    return () => clearTimeout(t);
  });
  $effect(() => {
    const pic = backdropUrl();
    const root = document.documentElement;
    const show = (on: boolean) => {
      root.style.setProperty('--flow-photo', on ? `url("${pic}")` : 'none');
      root.toggleAttribute('data-flow-photo', on);
    };
    if (!pic) return show(false);
    let gone = false;
    const img = new Image();
    img.src = pic;
    img.decode().then(
      () => !gone && show(true),
      () => !gone && show(false),
    );
    return () => (gone = true);
  });
  // Same breakpoint as the CSS: from 1024 px the sidebar shows; below it the bar of pages along the top.
  const wide = window.matchMedia('(min-width: 1024px)');
  let phone = $state(!wide.matches);
  // One glass on a phone (lib/glass): the panel on top hands its parts to the page's row. Wide again (a window made
  // wider; the owner, 2026-10-26: the tablet showed the album panel's glass twice), whatever was handed goes back, even
  // where the panel that handed it did not take it back itself.
  $effect(() => {
    glass.one = phone;
    if (!phone) glass.panel = null;
  });
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
  /** The phone's slim row of the mode switcher over the tabs (decision 0052, mock C), while it shows: its 34 px and the
   *  4 px under it (.bar .slim). Counted as 34, the glass under the bar sat 4 px into its gap (the owner, 2026-10-25). */
  const SLIM = 38;
  /** The mode switch in the bar or the sidebar (Settings' This device › Mode switch in the bar); else at the top of
   *  Settings. */
  const switchInBar = $derived(switchable() && prefs.modeSwitchInBar);
  const topBar = $derived(TOP_BAR + (switchInBar ? SLIM : 0));
  /** The mini-player's height with its margin (64 px and app.css's --gutter-x either side), kept free under the page
   *  and the toasts. */
  const MINI_SPACE = 80;
  const miniSpace = $derived(mini && playing() ? MINI_SPACE : 0);
  // The page's row (Symfonium's, decided 2026-10-15), floating over the page's top: back on the left, when this visit
  // has a step before this one; on the right, on a page with something to play, the pill (lib/TopPill): the filter
  // always, the page's View, and where it plays when nothing else leads there (no mini-player, no player beside the
  // page). The page starts under the row, so at rest nothing lies under it.
  const PLAYABLE = ['playlists', 'crates', 'library', 'downloads', 'local-albums'];
  // A part of the settings in the panel (lib/SettingsPanel), when the panel shows one.
  const settingsShown = $derived(SECTIONS.find((x) => x.id === settingsOf(panel.viewed))?.id ?? (null as SettingsSection | null));
  // What the panel showed when this page was entered (panel.arrived, see openedHere).
  $effect(() => {
    void page;
    panel.arrived = untrack(() => panel.viewed);
  });
  // Beside Settings' list a part is always open: the one last open in this browser, else the first; opened in place, so
  // Back leaves the page rather than closing it. Once it is known who is signed in (the server's parts are its
  // administrators'). Only while the address is Settings' own: leaving it (Back, a link) clears the part a moment before
  // `page` follows, and opening one then would write it into the next page's address.
  $effect(() => {
    if (!settingsSplit || settingsShown || openedHere() || !me.known || fromHash() !== 'settings') return;
    const parts = SECTIONS.filter((x) => (!x.app || inApp) && (!x.admin || me.admin));
    const part = parts.find((x) => x.id === prefs.settingsPart) ?? parts[0];
    if (part) untrack(() => viewInPlace(SETTINGS_VIEW + part.id));
  });
  $effect(() => {
    if (settingsShown) prefs.settingsPart = settingsShown;
  });
  // Beside Health's list a problem is always open: the one last open while it lasts, else the first (the worst).
  $effect(() => {
    if (!healthSplit || healthShown || openedHere() || !health.loaded || fromHash() !== 'health') return;
    const keys = problems().map((x) => x.key);
    const part = keys.includes(prefs.healthPart) ? prefs.healthPart : keys[0];
    if (part) untrack(() => viewInPlace(HEALTH_VIEW + part));
  });
  $effect(() => {
    if (healthShown) prefs.healthPart = healthShown;
  });
  // Beside Downloads' index a list is always open: what is synced where something is, else this device.
  $effect(() => {
    if (!downloadsSplit || downloadsShown || openedHere() || fromHash() !== 'downloads' || !downloads.known) return;
    untrack(() => viewInPlace(DOWNLOADS_VIEW + (kept.items.length ? 'synced' : 'device')));
  });
  // Beside Requests' index a list is always open: the waiting ones for those who answer them, else the account's own.
  $effect(() => {
    // Only once the account's rights are known (they say which list comes first).
    if (!requestsSplit || requestsShown || openedHere() || fromHash() !== 'requests' || !requestsData.view) return;
    untrack(() => viewInPlace(REQUESTS_VIEW + (answersRequests() ? 'waiting' : 'mine')));
  });
  // Beside Insights' list a view is always open: the one last open (its tab and section, which the views keep).
  $effect(() => {
    if (!insightsSplit || insightsShown || openedHere() || fromHash() !== 'insights') return;
    untrack(() => viewInPlace(INSIGHTS_VIEW + viewNow()));
  });
  // What the page's place holds (lib/Hold): the page, local mode's Home being another (a switch of the mode, held until
  // it is ready, never half drawn).
  const pageKey = $derived(page === 'home' && localMode() ? 'home:local' : page);
  let shownKey = $state(untrack(() => pageKey));
  /** The page whose content shows (lib/Hold): until the one asked for is ready, its row is there but not yet to be
   *  used. The bar's highlight and the row follow the tap at once (the owner, 2026-10-27: always responsive, never what
   *  was there before). */
  const shownPage = $derived(shownKey.split(':')[0] as typeof page);
  const pagePill = $derived(PLAYABLE.includes(page));
  // An icon button's label on a long-press or right-click, everywhere (lib/labels).
  $effect(installLabels);
  // ← and → as Previous and Next, held to wind (lib/keyWind).
  $effect(installKeyWind);
  // A link to another site asks first, Open or Copy link (lib/LinkSheet; the owner, 2026-10-24), wherever it is.
  let outLink = $state<{ url: string; name: string } | null>(null);
  $effect(() => {
    const tapped = (e: MouseEvent) => {
      if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      const a = (e.target as Element | null)?.closest?.('a[href]') as HTMLAnchorElement | null;
      if (!a) return;
      const to = new URL(a.href, location.href);
      if (!/^https?:$/.test(to.protocol) || to.origin === location.origin) return;
      e.preventDefault();
      outLink = { url: to.href, name: a.textContent?.trim() || to.host };
    };
    document.addEventListener('click', tapped, true);
    return () => document.removeEventListener('click', tapped, true);
  });
  // Each page starts with its search empty (the pill's field), once it shows (lib/navStep: the page before keeps its
  // words until then; back to a search brings its words back instead).
  $effect(() => {
    void page;
    untrack(() => leaveOnArrival({ search: 'clear' }, true));
  });
  // The page as asked and as shown, for a step's arrival (lib/navStep).
  $effect(() => {
    navPlace.page = page;
    navPlace.shownPage = shownPage;
  });
  // The title beside back gives way to the open search where it would be cut to a few letters or less (the owner,
  // 2026-10-25): measured as the field slides open (200 ms, after a page change 100 ms later), and as the window
  // changes; a title that fits beside the open field stays as it is. Library's view (its picker is never cut: the
  // field's glass slides over it) gives way the same where the two glasses would meet (the owner, 2026-10-26).
  let leadEl = $state<HTMLElement>();
  let titleAway = $state(false);
  $effect(() => {
    if (!pageView.searchOpen) return void (titleAway = false);
    const measure = () => {
      if (!leadEl || titleAway) return;
      const parts = [...leadEl.querySelectorAll<HTMLElement>('.page-name, .crumbs .up, .crumbs .here, .title, .views')];
      const cut = parts.reduce((n, e) => n + Math.max(0, e.scrollWidth - e.clientWidth), 0);
      const room = parts.reduce((n, e) => n + e.clientWidth, 0);
      // More than a few letters cut (about three, or a third of it: "Req…"), or no room at all.
      // Its content's edge, not its glass's: a narrowed glass lets the picker run out under the field.
      const edge = Math.max(leadEl.getBoundingClientRect().right, ...parts.map((e) => e.getBoundingClientRect().left + e.scrollWidth));
      const met = !!rightEl && edge > rightEl.getBoundingClientRect().left - 4;
      titleAway = parts.length > 0 && (met || cut > 36 || room < 48 || room < 0.7 * (room + cut));
    };
    const ts = [150, 350, 600].map((ms) => window.setTimeout(measure, ms));
    addEventListener('resize', measure);
    return () => {
      ts.forEach(clearTimeout);
      removeEventListener('resize', measure);
    };
  });
  // Where a page was scrolled to, kept with its step of the history: back there brings it back; any other way there
  // (its link, Home's button) starts at its top (the owner, 2026-10-25).
  let popped = false;
  $effect(() => {
    let t = 0;
    const keep = () => {
      clearTimeout(t);
      t = window.setTimeout(() => history.replaceState({ ...(history.state ?? {}), scroll: Math.round(scrollY) }, ''), 150);
    };
    const pop = () => (popped = true);
    addEventListener('scroll', keep, { passive: true });
    addEventListener('popstate', pop);
    return () => {
      clearTimeout(t);
      removeEventListener('scroll', keep);
      removeEventListener('popstate', pop);
    };
  });
  $effect(() => {
    void page;
    const back = popped;
    popped = false;
    const y = back ? Number(history.state?.scroll ?? 0) : 0;
    if (!y) return void window.scrollTo(0, 0);
    // Its content comes in a moment: tried again until it is long enough (four seconds at most: Home's sections load).
    let n = 0;
    let t = 0;
    const go = () => {
      window.scrollTo(0, y);
      if (Math.abs(scrollY - y) > 2 && n++ < 40) t = window.setTimeout(go, 100);
    };
    requestAnimationFrame(go);
    return () => clearTimeout(t);
  });
  // The page's device button gives way to the panel's (the owner, 2026-10-16): a list beside the page has its own.
  const panelDevice = $derived(sidebar && panel.viewed !== null && !playerInPanel && !settingsShown && !healthShown && !insightsShown && !requestsShown && !downloadsShown);
  const pillDevice = $derived(!playing() && !playerInPanel && !panelDevice);
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
  // Back is always there (the owner, 2026-10-23): a step back in this visit, else Home; only Home itself, opened first,
  // has none. On a phone the row always shows.
  const backHome = $derived(navAt === 0 && page !== 'home');
  /** The page has the row (its room at its top kept for it). */
  const pageHasRow = $derived(
    phone || navAt > 0 || backHome || pagePill || page === 'settings' || page === 'insights' || page === 'discover',
  );
  /** The room at the page's top as the page shown has it (lib/Hold: the page before stays until the new one is ready;
   *  the owner, 2026-10-28: its room changing under it made the content jump). */
  const rowOf = (p: string) => phone || navAt > 0 || p !== 'home';
  const shownHasRow = $derived(rowOf(shownPage));
  // Kept under the player over everything, which covers it (the owner, 2026-10-25: "since the full screen player is on
  // top, just don't hide these glass panels"; neither the page's room nor the glass goes as it opens).
  const pageRow = $derived(pageHasRow);
  // The panel on top's glass, handed over on a phone (lib/glass): the row shows it over the panel's layer.
  const handedNow = $derived(layerShown && phone ? glass.panel : null);
  // A row that goes slides up whole (a row along the bottom, down), out under the bar, still showing what it held, and
  // only then leaves (the owner, 2026-10-25: not its icons folding one by one, leaving a flat glass). The panel's glass
  // it held is kept for the slide.
  const ROW_SLIDE = 200;
  // Where the page keeps room for it, and on Home always: opened first beside the sidebar it keeps none, its pill (the
  // search, where it plays) floating over the page's top (the owner, 2026-10-29: "the entire glass on the home page"
  // gone, and the top-right search with it).
  const rowWanted = $derived(pageRow || !!handedNow || page === 'home');
  let rowKept = $state(untrack(() => rowWanted));
  let rowAway = $state(false);
  let keptGlass = $state<typeof handedNow>(null);
  $effect(() => {
    if (handedNow) keptGlass = handedNow;
  });
  $effect(() => {
    if (rowWanted) {
      rowKept = true;
      rowAway = false;
      return;
    }
    if (!untrack(() => rowKept)) return;
    rowAway = true;
    const t = window.setTimeout(() => {
      rowKept = false;
      rowAway = false;
      keptGlass = null;
    }, reducedMotion() ? 0 : ROW_SLIDE);
    return () => clearTimeout(t);
  });
  // Not kept for the slide once wide: the panel draws its own glass at once.
  const handed = $derived(handedNow ?? (rowAway && phone ? keptGlass : null));
  /** The layer under the top bar (an index page's panel) or over all of it (a playlist's, an album's, an artist's). */
  const underBar = $derived(phone && !!panelPage && !!(settingsShown || healthShown || insightsShown || requestsShown || downloadsShown));
  /** An index page and its panel share the row's place: going one level deeper (or back) changes what the glass holds,
   *  at once, instead of fading it in anew (the owner, 2026-10-25: "the top left and top right glass hide"). */
  const leadFade = $derived(reducedMotion() || (phone && !!panelPage && (underBar || !layerShown)) ? 0 : 150);
  /** The one pill's settings: the panel's where it handed them, else the page's. */
  const pagePillShown = $derived(pagePill || ['settings', 'discover', 'insights', 'health', 'requests', 'home', 'find'].includes(page));
  const pillNow = $derived(
    handed
      ? handed.pill()
      : pagePillShown
        ? {
            page: page,
            view: hasView(page),
            // Home's too (the owner, 2026-10-28: the device with nothing playing, as every page with the pill).
            device: (pagePill || page === 'home') && pillDevice,
            filter: (pagePill && !page.startsWith('local-') && page !== 'downloads') || page === 'discover',
            action: pageView.action,
          }
        : null,
  );
  // One glass of a row that stays, emptied (the owner, 2026-10-25: "it should indeed slide away"): as the row does, up
  // out under the bar whole (along the bottom, down), still showing what it held. A copy of it slides; the glass itself
  // goes at once (Search's icons' folding unseen under it).
  let rightEl = $state<HTMLElement>();
  const leadHas = $derived(!!handed || navAt > 0 || backHome || (!!panelPage && phone) || page === 'library');
  /** The same for the page shown (its room at the top, above). */
  const shownLeadHas = $derived(!!handed || navAt > 0 || shownPage !== 'home' || (!!panelPage && phone));
  const rightHas = $derived(!!(pillNow || handed?.extra));
  let had = { lead: false, right: false };
  // Before the page changes, while the glass still shows what it held.
  $effect.pre(() => {
    const now = { lead: leadHas, right: rightHas };
    const stays = rowWanted;
    untrack(() => {
      if (stays && rowKept && !rowAway) {
        if (had.lead && !now.lead && leadEl) slideAway(leadEl, false);
        if (had.right && !now.right && rightEl) slideAway(rightEl, true);
      }
      had = now;
    });
  });
  function slideAway(el: HTMLElement, hide: boolean) {
    const row = el.closest<HTMLElement>('.page-row');
    if (!row || reducedMotion()) return;
    const [r, rr] = [el.getBoundingClientRect(), row.getBoundingClientRect()];
    if (!r.width) return;
    const copy = el.cloneNode(true) as HTMLElement;
    copy.setAttribute('aria-hidden', 'true');
    copy.inert = true;
    Object.assign(copy.style, { position: 'absolute', left: `${r.left - rr.left}px`, top: `${r.top - rr.top}px`, width: `${r.width}px`, height: `${r.height}px`, margin: '0', pointerEvents: 'none' });
    row.append(copy);
    if (hide) el.style.visibility = 'hidden';
    const dy = row.classList.contains('at-bottom') ? innerHeight - r.top + 16 : -(r.bottom + 16);
    const gone = () => copy.remove();
    copy.animate([{ transform: 'none' }, { transform: `translateY(${dy}px)` }], { duration: ROW_SLIDE, easing: 'ease-in' }).finished.then(gone, gone);
  }
  // The layout below 1024 px, the user's (Display; the owner, 2026-10-25): the bar of pages along the top or the
  // bottom, sliding away while scrolling down (and back up) or always shown, panels under it either way; the controls
  // (back, the title, the pill) at the top or just above the mini-player, following it as it comes and goes.
  const navBottom = $derived(phone && prefs.navAt === 'bottom');
  const miniTop = $derived(phone && prefs.miniAt === 'top');
  const leadBottom = $derived(phone && prefs.navControlsAt === 'bottom');
  const rightBottom = $derived(phone && prefs.contentControlsAt === 'bottom');
  let navAway = $state(false);
  $effect(() => {
    // Along the bottom it never slides away (the owner, 2026-10-25).
    if (!phone || navBottom || !prefs.navHides) return void (navAway = false);
    const last = new WeakMap<object, number>();
    const scrolled = (e: Event) => {
      const el = e.target === document ? document.scrollingElement : (e.target as Element);
      if (!el) return;
      const y = el.scrollTop;
      const was = last.get(el) ?? y;
      last.set(el, y);
      if (y < 80) navAway = false;
      else if (y - was > 6) navAway = true;
      else if (was - y > 6) navAway = false;
    };
    addEventListener('scroll', scrolled, { capture: true, passive: true });
    return () => removeEventListener('scroll', scrolled, { capture: true });
  });
  // In a panel whose action bar sticks (lib/glass's `below`), the bar is away exactly while it sticks, scrolling up
  // too (the owner, 2026-10-25).
  // And while a library item is open, where the user wants it so (Display; its panel hands a `below`).
  const navHidden = $derived(
    !navBottom && (navAway || (phone && !!handedNow?.below && (prefs.navHidesOnItem || (prefs.stickyActions && handedNow.below())))),
  );
  // Somewhere else: the bar shows again.
  $effect(() => {
    void page;
    void panel.viewed;
    navAway = false;
  });
  $effect(() => {
    const root = document.documentElement.style;
    const shown = phone && !navHidden ? topBar : 0;
    const up = miniTop ? miniSpace : 0;
    const down = miniTop ? 0 : miniSpace;
    // Along each edge, from it inwards: the bar, the mini-player, the controls. The page's room (kept while the bar
    // slides away, so the page does not jump), and where things float as the bar shows now.
    root.setProperty('--top-h', `${(phone && !navBottom ? topBar : 0) + up}px`);
    root.setProperty('--bar-top', navBottom ? '0px' : `${shown}px`);
    root.setProperty('--top-now', `${(navBottom ? 0 : shown) + up}px`);
    root.setProperty('--nav-b', navBottom ? `${shown}px` : '0px');
    // The mini-player along the top floats over what scrolls, as along the bottom: room before the first thing only.
    root.setProperty('--mini-top', `${up}px`);
    // The bar's own room at a panel's top or foot (kept while it slides away): panels fill the screen under the bar,
    // which is drawn over them, so what scrolls goes on behind its glass as on a page (the owner, 2026-10-25).
    root.setProperty('--bar-room', phone && !navBottom ? `${topBar}px` : '0px');
    root.setProperty('--bar-room-b', navBottom ? `${topBar}px` : '0px');
    root.setProperty('--bottom-h', `${down + (navBottom ? topBar : 0)}px`);
    root.setProperty('--bottom-now', `${down + (navBottom ? shown : 0)}px`);
    // No room kept above the page where the left glass is empty (the owner, 2026-10-27): the page starts at the top, the
    // pill floating over its right.
    root.setProperty('--glass-top', (phone && leadBottom && rightBottom) || !shownLeadHas ? '0px' : 'var(--glass-room)');
    root.setProperty('--glass-bottom', leadBottom || rightBottom ? 'var(--glass-room)' : '0px');
  });
  // The playing cover's colour, for the floating controls' surface (app.css --float-bg): the mini-player's. Unless the
  // user said not (Settings › Appearance › Cover colours): Cratarr's own then, as with nothing playing.
  $effect(() => {
    document.documentElement.style.setProperty('--now-accent', (prefs.coverColourGlass && art.accent) || 'var(--accent)');
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
  const sideWidth = $derived(split ? Math.max(PLAYER_MIN, area.w - SETTINGS_LIST) : playerWidth);
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

  // The page follows the address in the same step as the panel (lib/panel's followAddress), not after it.
  $effect(() =>
    followAddress(() => {
      page = fromHash();
      moreOpen = false;
    }),
  );

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
    if (!inApp || openedAway || reach.answered || !reach.away || localMode()) return;
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
      ? { ok: false, text: (reach.kept ? 'Offline · as last seen' : 'Cratarr server unreachable') + (later.list.length ? ` · ${later.list.length} to send` : '') }
      : !status
        ? null
        : status.connected
          ? status.problems.length
            ? { ok: false, text: status.problems[0] }
            : { ok: true, text: `Connected to ${status.server_name ?? 'Jellyfin'}` }
          : { ok: false, text: 'Jellyfin unreachable' },
  );
  $effect(() => {
    serverConnection.value = connection;
  });
  const MAIN: Page[] = $derived(localMode() ? [HOME, ...LOCAL] : [HOME, CRATES, PLAYLISTS, LIBRARY, DISCOVER]);
  // The sidebar's direct links under Library and Discover (the owner, 2026-10-29): what each one's picker beside back
  // offers, folded away with the sidebar; Spotify's lists as one, to the first of them.
  loadSpotify();
  type Sub = { id: string; label: string; on: boolean; pick: () => void };
  const kindShown = $derived(kindNow());
  const librarySubs = $derived<Sub[]>(recordKinds().map((k) => ({ id: k.id, label: k.label, on: kindShown === k.id, pick: () => chooseKind(k.id) })));
  const discoverSubs = $derived<Sub[]>([
    ...DISCOVER_MODES.map((m) => ({ id: m.value, label: m.label, on: prefs.discoverMode === m.value, pick: () => (prefs.discoverMode = m.value) })),
    ...(spotify.state?.shown
      ? [{ id: 'spotify', label: 'Spotify', on: inSpotify(), pick: () => (prefs.discoverMode = inSpotify() ? prefs.discoverMode : (SPOTIFY_VIEWS[0].value as DiscoverMode)) }]
      : []),
  ]);
  const subsOf = (id: string): Sub[] => (id === LIBRARY.id ? librarySubs : id === DISCOVER.id ? discoverSubs : []);
  // One section open at a time (0.10; the owner, 2026-10-30): the page's own, or the one tapped open. A tap on a
  // section's name only opens it (no page change; one of its links goes there); folded, the sidebar unfolds for it
  // (peek). One opened by hand folds again on a tap or the pointer elsewhere, and the page's comes back.
  let opened = $state<string | null>(null);
  const pageSection = $derived(subsOf(page).length ? page : null);
  const openSection = $derived(opened ?? pageSection);
  function sectionTap(e: MouseEvent, id: string) {
    e.preventDefault();
    if (prefs.navFolded && !peek) peek = true;
    opened = opened === id || id === pageSection ? null : id;
  }
  // Folded on the tap's click, not its press (the owner, 2026-10-30: a tap below landed elsewhere): folding moves what
  // lies under the finger, and between press and release that sent the click to another link. At the click, what it
  // hit is already settled.
  $effect(() => {
    if (opened === null) return;
    const away = (e: MouseEvent) => {
      if (!(e.target as Element | null)?.closest?.(`[data-section="${opened}"]`)) opened = null;
    };
    addEventListener('click', away, true);
    return () => removeEventListener('click', away, true);
  });
  // In sight (the owner, 2026-10-30), where the sidebar is too low for all of it: a section opened by hand; else the
  // page's end of it, the top for the pages from Home to Discover, the bottom for those at its foot (Settings…).
  let sidebarEl = $state<HTMLElement>();
  $effect(() => {
    const el = sidebarEl;
    const id = opened;
    const below = APART.some((p) => p.id === page);
    if (!el || el.scrollHeight <= el.clientHeight) return;
    const behavior: ScrollBehavior = reducedMotion() ? 'auto' : 'smooth';
    // After the section's unfolding (its slide), so its whole height is known.
    const t = setTimeout(() => {
      if (id) el.querySelector(`[data-section="${id}"]`)?.scrollIntoView({ block: 'nearest', behavior });
      else el.scrollTo({ top: below ? el.scrollHeight : 0, behavior });
    }, id ? 160 : 0);
    return () => clearTimeout(t);
  });
  // Gone to (one of its links, or the page changed otherwise): the page's own section is the open one.
  $effect(() => {
    void page;
    untrack(() => (opened = null));
  });
  // Apart, at the bottom of the sidebar: Insights beside Health, then Settings.
  // Health is the library's, so its administrators' (decision 0051). In Local mode, Settings only.
  const mayRequest = $derived(me.admin || me.permissions.request || me.permissions.manage_requests);
  const APART: Page[] = $derived(
    localMode() ? [SETTINGS] : [...(mayRequest ? [REQUESTS] : []), ...(inApp || installed ? [DOWNLOADS] : []), INSIGHTS, ...(me.admin ? [HEALTH] : []), SETTINGS],
  );
  // The global search finds the pages shown and their parts, and Settings' (lib/everywhere).
  provideEverywhere(() => [...MAIN, ...APART]);
  // A switch of the mode (decision 0052) goes to the first page of the mode it went to, unless the page is one both
  // have (Settings, Home: local mode's its own, lib/localHome).
  $effect(() => {
    const local = localMode();
    // No visuals in local mode (the owner: "In local mode, the visuals feature is not supported").
    if (local) untrack(() => (visuals.open = false));
    const p = untrack(() => page);
    if (p === 'settings' || p === HOME.id) return;
    if (local && !p.startsWith('local-')) location.hash = HOME.href;
    else if (!local && p.startsWith('local-')) location.hash = LIBRARY.href;
  });
  const ALL = $derived([...MAIN, ...APART]);
  /** The page's search has its globe (lib/TopPill): everywhere's shown in the page's place. */
  const scoped = $derived(page !== 'home' && page !== 'find');
  /** Everywhere's results in the page's place: on a page's Everywhere tab, and on Home whenever it searches (it has
   *  nothing of its own to find; the owner, 2026-10-25: its results came after all of Home, out of sight). */
  const everywhereShown = $derived(pageView.query.trim() !== '' && (page === 'home' || (scoped && globalSearch.scope === 'everywhere')));
  // As many of the main pages as fit, each tab at least BAR_SLOT_PX wide (the owner, 2026-10-25: Discover in the bar where
  // there is room): four and More on a phone, all five on a tablet. The pages apart (Requests, Insights, Health,
  // Settings…) stay behind More ("requests is of the other category"), unless every page fits in five (Local mode).
  let barWidth = $state(innerWidth);
  $effect(() => {
    const on = () => (barWidth = innerWidth);
    addEventListener('resize', on);
    return () => removeEventListener('resize', on);
  });
  const slots = $derived(Math.max(BAR_SLOTS, Math.floor(barWidth / BAR_SLOT_PX)));
  const bar = $derived(ALL.length <= BAR_SLOTS ? ALL : MAIN.slice(0, Math.min(MAIN.length, slots - 1)));
  const overflow = $derived(ALL.filter((p) => !bar.includes(p)));
  /** More's columns: at most the bar's own, its rows evenly filled (the owner, 2026-10-26: six pages on a phone are two
   *  rows of three, not five and one). */
  const moreCols = $derived.by(() => {
    const most = Math.max(1, bar.length + 1);
    const rows = Math.ceil(overflow.length / most);
    return Math.max(1, Math.ceil(overflow.length / Math.max(1, rows)));
  });
  const overflowActive = $derived(overflow.some((p) => p.id === page));
</script>

{#if outLink}{@const l = outLink}<LinkSheet url={l.url} name={l.name} onclose={() => (outLink = null)} />{/if}

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
    onpointerleave={(e) => e.pointerType === 'mouse' && navDrag === null && ((peek = false), (opened = null))}
  >
    <aside class="sidebar" class:peek bind:this={sidebarEl}>
      <!-- The mode switcher with a local folder on (decision 0052, mock B); otherwise the mark and the connection line. -->
      {#if switchInBar}
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
          {@const subs = subsOf(p.id)}
          {#if subs.length}
            <!-- A section (Library, Discover): its name opens it, its links go there. -->
            <div class="section" data-section={p.id}>
              <a
                href={keepViewed(p.href)}
                aria-current={page === p.id ? 'page' : undefined}
                aria-expanded={openSection === p.id}
                onclick={(e) => sectionTap(e, p.id)}
                ><Icon name={p.icon} /><span class="label">{p.label}</span>{#if p.id === 'library' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</a
              >
              {#if openSection === p.id}
                <div class="subs" role="group" aria-label="{p.label} shows" transition:slide={{ duration: reducedMotion() ? 0 : 150 }}>
                  {#each subs as s (s.id)}
                    <a class="sub" href={keepViewed(p.href)} aria-current={page === p.id && s.on ? 'true' : undefined} onclick={() => ((peek = false), (opened = null), s.pick())}>{s.label}</a>
                  {/each}
                </div>
              {/if}
            </div>
          {:else}
            <a href={keepViewed(p.href)} aria-current={page === p.id ? 'page' : undefined} onclick={(e) => ((peek = false), samePage(e, p.id))}
              ><Icon name={p.icon} /><span class="label">{p.label}</span></a
            >
          {/if}
        {/each}
      </nav>

      <div class="foot">
        {#each APART as p (p.id)}
          <a href={keepViewed(p.href)} aria-current={page === p.id ? 'page' : undefined} onclick={(e) => ((peek = false), samePage(e, p.id))}><Icon name={p.icon} /><span class="label">{p.label}</span></a>
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

  <div class="content" class:with-side={besideDrawn} style:--player-w="{sideWidth}px" bind:this={contentEl}>
    <!-- Its room under the glass kept while a panel covers it on a phone (the owner, 2026-10-25: opening a list on Requests
         shifted the page up under the coming panel). -->
    <div class="page" class:under-row={shownHasRow} class:away={everywhereShown} bind:this={pageEl}>
    <SheetArea area={() => pageEl}>
    <!-- One commit (lib/Hold; the owner, 2026-10-26, "Old page stays, then switch"): the page before stays until the new
         one is ready, and the bar's highlight moves with it. -->
    <Hold key={pageKey} bind:shown={shownKey}>
    {#if page === 'settings'}
      <Settings />
    {:else if page === 'health' && me.admin}
      <Health />
    {:else if page === 'health' && !me.known}
      <!-- Whether they may see it is not known yet: not another page first. -->
      <PageLoading />
    {:else if page === 'downloads' && (inApp || installed)}
      <Downloads />
    {:else if page === 'library'}
      <Library />
    {:else if page === 'local-albums'}
      <LocalAlbums />
    {:else if status && !status.music && me.admin}
      <p class="no-music">No Music library is set. Choose one in Settings → Libraries.</p>
    {:else if page === 'insights'}
      <Insights />
    {:else if page === 'crates'}
      <Crates />
    {:else if page === 'discover'}
      <Discover />
    {:else if page === 'requests'}
      <Requests />
    {:else if page === 'home'}
      <!-- Local mode's Home is its own (lib/localHome): a switch of the mode opens it anew, held as a page is (pageKey). -->
      {#key localMode()}<Home />{/key}
    {:else if page === 'find'}
      <Find />
    {:else}
      <Playlists />
    {/if}
    </Hold>
    <!-- Global search at the end of what the page's own search leaves (decision 0072), but the kind the page shows. -->
    {#if everywhereShown}<GlobalResults except={pageKinds(page)} />{/if}
    </SheetArea>
    </div>
    {#if split}
      <div class="grip line" aria-hidden="true"></div>
    {:else if besideDrawn}
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
    {#if besideDrawn}
      <!-- The mini-player floats over the panel's end too: its lists leave room for it (--mini-h). -->
      <aside class="player-side" class:whole={split} aria-label={playerInPanel ? 'Player' : settingsShown ? 'Settings' : healthShown ? 'Health' : insightsShown ? 'Insights' : requestsShown ? 'Requests' : downloadsShown ? 'Downloads' : 'Playlist'} style:--mini-h={`${miniTop ? 0 : miniSpace}px`} bind:this={sideEl}>
        <SheetArea area={() => sideEl}>
          <!-- One commit (lib/Hold): what the panel showed stays until the item opened is ready. -->
          <Hold key={panel.viewed} bind:shown={panel.shown} onshown={() => sidebar && letGoHeld()}>
          {#if visualsBeside}<Visuals docked onclose={() => (visuals.open = false)} />{:else if playerInPanel}<NowPlaying place="side" />{:else if split && panel.viewed === null}<!-- Its part comes in a moment. -->{:else if settingsShown}<SettingsPanel section={settingsShown} beside={settingsSplit} />{:else if healthShown}<HealthPanel problem={healthShown} beside={healthSplit} />{:else if insightsShown}<InsightsPanel view={insightsShown} beside={insightsSplit} />{:else if requestsShown}<RequestsPanel list={requestsShown} beside={requestsSplit} />{:else if downloadsShown}<DownloadsPanel list={downloadsShown} beside={downloadsSplit} />{:else}<PlaylistView />{/if}
          </Hold>
        </SheetArea>
      </aside>
    {/if}
  </div>

  {#if layerDrawn}
    <div class="playlist-layer" class:under-bar={underBar} role="dialog" aria-label="Playlist" style:--mini-h={`${(miniTop ? 0 : miniSpace) + (navBottom ? topBar : 0)}px`} transition:fade={{ duration: reducedMotion() || handingNow ? 0 : 200 }}>
      <Hold key={panel.viewed} bind:shown={panel.shown} seeThrough onshown={() => playlistLayer && letGoHeld()}>{#if settingsShown}<SettingsPanel section={settingsShown} />{:else if healthShown}<HealthPanel problem={healthShown} />{:else if insightsShown}<InsightsPanel view={insightsShown} />{:else if requestsShown}<RequestsPanel list={requestsShown} />{:else if downloadsShown}<DownloadsPanel list={downloadsShown} />{:else}<PlaylistView full />{/if}</Hold>
    </div>
  {:else if navPlace.layerHeld && !besideDrawn}
    <!-- A panel closed for another page (a breadcrumb's): its layer, empty, over the page until the page gone to shows
         (lib/navStep), not the page before for a moment. -->
    <div class="playlist-layer" aria-hidden="true" out:fade={{ duration: reducedMotion() ? 0 : 200 }}></div>
  {/if}
  {#if mini}
    <div class="mini-dock" class:over={playlistLayer} class:at-top={miniTop}><MiniPlayer /></div>
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
  {#snippet morePages()}
    <!-- More (the owner, 2026-10-26): the bar itself grows, full width, and the other pages are its tabs there, in
         columns under (along the bottom: over) the bar's own, as they are drawn. -->
    {#if moreOpen}
      <div
        class="more-tabs"
        id="more-pages"
        role="group"
        aria-label="More pages"
        style:--cols={moreCols}
        transition:slide|global={{ duration: reducedMotion() ? 0 : SHEET_IN, easing: decelerate }}
      >
        {#each overflow as p (p.id)}
          <a href={p.href} aria-current={page === p.id ? 'page' : undefined} onclick={(e) => barTap(e, p.id)}
            ><span class="icon"><Icon name={p.icon} />{#if p.id === 'library' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</span><span>{p.label}</span></a
          >
        {/each}
      </div>
    {/if}
  {/snippet}
  {#snippet leadPart()}
      {#if handed}
        <span class="lead" class:title-away={titleAway} bind:this={leadEl} in:fade={{ duration: leadFade }}>{@render handed.left()}</span>
      {:else}
        <span class="lead" class:title-away={titleAway} bind:this={leadEl} in:fade={{ duration: leadFade }}>
          {#if navAt > 0 || backHome || globalSearch.all}
            <!-- Out of a search's Show all first (it is a step of the history), then back as ever. -->
            <span class="navpill"><button aria-label="Back" title="Back" onclick={() => (globalSearch.all || !backHome ? history.back() : (location.hash = keepViewed(HOME.href)))}><Icon name="back" size={22} /></button></span>
          {/if}
          <!-- Wherever the bar of pages is along the top (under 1024 px, as the CSS; the owner, 2026-10-25): from there the
               sidebar of pages names it. -->
          {#if panelPage && phone}<h1 class="page-name">{panelPage.label}</h1>{/if}
          <!-- What the page shows, on the glass's extension as a control page's name (the owner, 2026-10-25): the
               Library's kind of record, Discover's mode (the owner, 2026-10-29). -->
          {#if page === 'library'}<RecordsPicker />{:else if page === 'discover'}<DiscoverPicker />{/if}
        </span>
      {/if}
  {/snippet}
  {#snippet rightPart()}
      <!-- Settings' pill: its search and About, its own button (the owner, 2026-10-24: one pill as on every page);
           Discover's: its search and Look again (the owner, 2026-10-24). The panel's where it handed its glass. -->
      {#if pillNow || handed?.extra}
        <span class="right" bind:this={rightEl}>
          {#if pillNow}<SheetArea area={() => pageEl}><TopPill {...pillNow} /></SheetArea>{/if}{#if handed?.extra}{@render handed.extra()}{/if}
        </span>
      {/if}
  {/snippet}
  {#if rowKept}
    <!-- The page's glass, or on a phone the panel's on top (lib/glass): one row, one pill whose settings change. Below
         1024 px each half at the edge its setting says (Display: Navigation controls, Content controls). -->
    {#if !leadBottom || !rightBottom}
      <div inert={page !== shownPage || (layerShown && !handed) || undefined} class="page-row" class:away={rowAway} class:beside={besideDrawn && !handed} class:handed={!!handed} class:below={!!handed?.below?.()} class:steady={phone && !!panelPage && !moreOpen} style:--player-w="{sideWidth}px">
        {#if !leadBottom}{@render leadPart()}{:else}<span></span>{/if}
        {#if !rightBottom}{@render rightPart()}{/if}
      </div>
    {/if}
    {#if leadBottom || rightBottom}
      <div inert={page !== shownPage || (layerShown && !handed) || undefined} class="page-row at-bottom" class:away={rowAway} class:handed={!!handed} class:steady={phone && !!panelPage && !moreOpen}>
        {#if leadBottom}{@render leadPart()}{:else}<span></span>{/if}
        {#if rightBottom}{@render rightPart()}{/if}
      </div>
    {/if}
  {/if}
  {#if overlayDrawn}
    <!-- Over everything, as Finamp's: it slides up when it opens (300 ms, easeInOutQuad) and fades when it closes
         (300 ms, linear); come from beside the page, it is there at once over it (lib/handover). -->
    <div class="overlay" role="dialog" aria-label="Player" {@attach overlayShown} in:fly={{ y: innerHeight, duration: reducedMotion() || handingNow ? 0 : 300, easing: quadInOut }} out:fade={{ duration: reducedMotion() ? 0 : 300, easing: linear }}>
      <NowPlaying place="overlay" />
    </div>
  {/if}

  {#if about.open}<AboutSheet onclose={() => (about.open = false)} />{/if}

  {#if moreOpen}
    <!-- The page under the bar grown with the other pages dims; a tap there closes it. -->
    <button class="scrim" aria-label="Close menu" onclick={() => (moreOpen = false)} transition:fade|global={{ duration: SHEET_OUT }}></button>
  {/if}

  {#if visuals.open && !visualsBeside}
    <Visuals onclose={() => (visuals.open = false)} />
  {/if}
  <Toasts />
  <ManyConfirm />
<LabelTip />
  <ProgressBar />

  <nav class="bar" class:bottom={navBottom} class:away={navHidden} aria-label="Pages" onclickcapture={collapse}>
    <!-- The mode switcher's slim row over the tabs (decision 0052, mock C), with a local folder on; under them when the bar
         is along the bottom (the owner, 2026-10-25), the tabs nearest the page either way. -->
    {#if switchInBar && !navBottom}<div class="slim"><ModeSwitcher slim {connection} /></div>{/if}
    {#if navBottom}{@render morePages()}{/if}
    <div class="tabs">
    {#each bar as p (p.id)}
      <a href={p.href} aria-current={page === p.id ? 'page' : undefined} onclick={(e) => barTap(e, p.id)}
        ><span class="icon"><Icon name={p.icon} />{#if p.id === 'library' && waiting}<span class="count" aria-label="{waiting} converting">{waiting}</span>{/if}</span><span>{p.label}</span></a
      >
    {/each}
    {#if overflow.length}
      <button
        class:active={overflowActive}
        aria-expanded={moreOpen}
        aria-controls="more-pages"
        onclick={() => (moreOpen = !moreOpen)}><Icon name="more" /><span>More</span></button
      >
    {/if}
    </div>
    {#if !navBottom}{@render morePages()}{/if}
    {#if switchInBar && navBottom}<div class="slim"><ModeSwitcher slim up {connection} /></div>{/if}
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
    bottom: calc(104px + var(--nav-b, 0px) + env(safe-area-inset-bottom));
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
    inset: auto var(--gutter-x) calc(var(--gutter-x) + var(--nav-b, 0px) + env(safe-area-inset-bottom));
    z-index: 22;
    transition:
      top 200ms ease,
      bottom 200ms ease;
  }

  /* Along the top (Display's Mini player), under the bar there. */
  .mini-dock.at-top {
    top: calc(var(--bar-top, 0px) + env(safe-area-inset-top) + var(--gutter-x));
    bottom: auto;
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

  /* A panel of Requests, Downloads, Insights, Health or Settings on a phone: under the top bar, not over it (the owner,
     2026-10-25); a playlist's, an album's or an artist's still covers it all. */
  .playlist-layer.under-bar {
    /* The page's colour, as beside the page (.player-side); a playlist's layer keeps the cards' (the owner, 2026-10-25). */
    background: var(--bg);
  }

  /* The page's row: back and the pill floating over the page's top, which starts under them (.under-row); under the page
     bar on a phone, beside the navigation and left of the panel on a wide window. Over the page and its bar, under a
     viewed playlist. */
  .page-row {
    position: fixed;
    top: calc(var(--top-now, 0px) + env(safe-area-inset-top) + var(--gutter-y));
    left: var(--gutter-x);
    right: var(--gutter-x);
    z-index: 20;
    display: flex;
    justify-content: space-between;
    /* Back stays round when what is beside it grows a row. */
    align-items: flex-start;
    /* Never touching: back and what is beside it (the owner, 2026-10-23). */
    gap: 8px;
    pointer-events: none;
  }

  .page-row > :global(*) {
    pointer-events: auto;
  }

  /* Back, and the page's name beside it where the page is an index (Requests, Downloads, Insights, Health, Settings). */
  /* Embedded always as with its extension (the owner, 2026-10-26: the glass moved a few pixels between the Library,
     with its view picker's thin glass, and Home, without): the extension's 2 px, whether it shows or not. */
  .lead,
  .right {
    padding: 2px;
  }

  .lead {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  /* A title in the glass (the owner, 2026-10-25, mockup 3: content scrolling under a bare title made it hard to read):
     the page's name or the panel's breadcrumbs on a thinner glass than the button's, more light through, the button's
     round glass in front of it. */
  .lead:has(:global(.page-name, .crumbs, .title, .views)) {
    padding: 2px 18px 2px 2px;
    border-radius: 26px;
    background: color-mix(in srgb, var(--float-bg) 45%, transparent);
    -webkit-backdrop-filter: blur(10px) saturate(1.3);
    backdrop-filter: blur(10px) saturate(1.3);
    box-shadow: 0 2px 10px rgb(0 0 0 / 0.18);
  }

  .lead:has(:global(.page-name, .crumbs, .title, .views)) :global(.navpill) {
    box-shadow: none;
  }

  /* The search open with no room for the title: only the round button, its thin glass gone with the title. */
  .lead.title-away :global(:is(.page-name, .crumbs, .title, .views)) {
    display: none;
  }

  .lead.title-away {
    padding: 2px;
    background: none;
    box-shadow: none;
    -webkit-backdrop-filter: none;
    backdrop-filter: none;
  }

  .lead.title-away :global(.navpill) {
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .page-name {
    margin: 0;
    overflow: hidden;
    font-size: 22px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Going: up out under the bar, whole (a row along the bottom: down); nothing in it takes a tap meanwhile. */
  .page-row.away {
    transform: translateY(calc(-100% - var(--top-now, 0px) - env(safe-area-inset-top) - 16px));
    transition: transform 200ms ease-in;
    pointer-events: none;
  }

  .page-row.at-bottom.away {
    transform: translateY(calc(100% + var(--bottom-now, 0px) + env(safe-area-inset-bottom) + 16px));
  }

  .page-row.away > :global(*) {
    pointer-events: none;
  }

  /* Over the panel's layer while it holds the panel's glass (lib/glass). */
  .page-row.handed {
    z-index: 22;
    transition: transform 150ms ease;
  }

  /* The panel's action bar has the top: its glass slides just under it (52 px), as its own pills did (the owner,
     2026-10-25: "the top panel used to slide down when this sticky action bar took its space"). */
  .page-row.below {
    transform: translateY(52px);
  }

  /* An index page's glass on a phone stays over its panel's layer as that fades out on closing, instead of showing
     through it (the owner, 2026-10-25: "the top glass and title show a fade in"); under the More sheet while it is open. */
  .page-row.steady {
    z-index: 22;
  }

  .page-row .right {
    display: flex;
    align-items: flex-start;
    justify-content: flex-end;
    gap: 8px;
    min-width: 0;
  }

  /* The pill's work unfolded (lib/TopPill): the right side takes the row's room. */
  .page-row .right:has(:global(.pill-row.wide)) {
    flex: 1 1 auto;
  }

  .page-row.beside {
    right: calc(var(--player-w) + 10px + var(--gutter-x));
  }

  .page.under-row {
    padding-top: var(--glass-top);
  }

  /* Back, round as the pill (lib/TopPill). */
  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
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
    padding-bottom: calc(var(--bottom-h, 0px) + var(--glass-bottom, 0px) + env(safe-area-inset-bottom));
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

  .bar {
    transition: transform 200ms ease;
  }

  /* Along the bottom (Display's Navigation bar): its line on top, the phone's bottom inset under it. */
  .bar.bottom {
    inset: auto 0 0;
    padding: 6px 8px calc(6px + env(safe-area-inset-bottom));
    border-top: 1px solid var(--border);
    border-bottom: 0;
  }

  /* Its glass carried on past the window's foot: Safari on an iPhone shows the page under its own bar there, below
     ours (the owner, 2026-10-27: an empty band of the page under the bar). */
  .bar.bottom::after {
    content: '';
    position: absolute;
    inset: 100% 0 auto;
    height: 50vh;
    background: inherit;
    backdrop-filter: inherit;
    pointer-events: none;
  }

  /* Scrolled away (Display's Hide when scrolling): off its edge. */
  .bar.away {
    transform: translateY(-100%);
  }

  .bar.bottom.away {
    transform: translateY(100%);
  }

  /* Every panel under the bar of pages, never over it (the owner, 2026-10-25): the panel fills the screen, the bar is
     drawn over it (its glass shows what scrolls behind it), and the panel's content leaves the bar's room. */
  .bar {
    z-index: 24;
  }

  /* The controls just above the mini-player (Display's Controls), following it as it comes and goes. */
  .page-row.at-bottom {
    top: auto;
    /* With the keyboard up, just above it (lib/keyboard's --kb): its search field stays in view. */
    bottom: max(calc(var(--bottom-now, 0px) + env(safe-area-inset-bottom) + var(--gutter-y)), calc(var(--kb, 0px) + var(--gutter-y)));
  }

  .page-row {
    transition:
      top 200ms ease,
      bottom 200ms ease;
  }

  .bar .tabs {
    display: flex;
    justify-content: space-around;
  }

  /* The other pages, in the bar's columns: each as wide as a tab, under the bar's own (over them along the bottom),
     a line between. */
  .bar .more-tabs {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    justify-items: center;
    margin-top: 4px;
    padding-top: 4px;
    border-top: 1px solid var(--border);
  }

  .bar.bottom .more-tabs {
    order: -1;
    margin: 0 0 4px;
    padding: 0 0 4px;
    border-top: 0;
    border-bottom: 1px solid var(--border);
  }

  .bar .more-tabs a {
    width: 100%;
  }

  .bar .slim {
    padding: 0 4px 4px;
  }

  .bar.bottom .slim {
    padding: 4px 4px 0;
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
    color: var(--accent-ink);
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

  .sidebar nav a:hover,
  .foot a:hover {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  .sidebar nav a[aria-current='page'],
  .foot a[aria-current='page'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  /* The direct links under Library and Discover (the owner, 2026-10-29), in line with their page's name; the one its
     picker shows in the accent's ink, the page itself lit above it. */
  .sidebar nav .section {
    display: grid;
  }

  .sidebar nav .subs {
    display: grid;
    gap: 2px;
    margin: 2px 0 4px;
  }

  .sidebar nav a.sub {
    min-height: 34px;
    padding-left: 48px;
    font-size: 14px;
  }

  .sidebar nav a.sub[aria-current='true'] {
    color: var(--accent-ink);
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
      left: calc(var(--nav-w) + var(--gutter-x));
    }

    /* Tap to play in the middle of the page's area, not the window's (the owner, 2026-10-24). */
    .tap-to-play {
      left: calc(var(--nav-w) + (100% - var(--nav-w)) / 2);
    }

    /* A playlist over the page covers the navigation too: the mini-player floats over it with the same space on both
       sides, never pushed to one side by a navigation it is not beside (the owner, 2026-10-17). */
    .mini-dock.over {
      left: var(--gutter-x);
    }

    .bar,
    .scrim {
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
    color: var(--danger-ink);
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

    .folded .sidebar:not(.peek) :is(.version, .subs) {
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
