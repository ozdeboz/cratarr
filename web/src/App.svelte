<script lang="ts">
  import Playlists from './Playlists.svelte';
  import Repairs from './Repairs.svelte';
  import Settings from './Settings.svelte';
  import Incoming from './Incoming.svelte';
  import Insights from './Insights.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import SourcePicker from './lib/SourcePicker.svelte';
  import NowPlaying from './lib/NowPlaying.svelte';
  import { player } from './lib/live.svelte';
  import { poll } from './lib/live.svelte';
  import { known } from './lib/sources.svelte';
  import { getStatus, type Status } from './lib/api';
  import { prefs } from './lib/prefs.svelte';

  type Page = { id: string; href: string; label: string; icon: IconName };
  // Main pages in order; Settings sits apart at the bottom of the sidebar.
  const PLAYLISTS: Page = { id: 'playlists', href: '#/', label: 'Playlists', icon: 'playlists' };
  // Shown only when some source has an incoming library (see the status call below).
  const INCOMING: Page = { id: 'incoming', href: '#/incoming', label: 'Incoming', icon: 'incoming' };
  const INSIGHTS: Page = { id: 'insights', href: '#/insights', label: 'Insights', icon: 'insights' };
  const REPAIRS: Page = { id: 'repairs', href: '#/repairs', label: 'Repairs', icon: 'repairs' };
  const SETTINGS: Page = { id: 'settings', href: '#/settings', label: 'Settings', icon: 'settings' };

  // Phones get a bottom bar. Up to five fit; beyond that the first four stay and the rest go under "More".
  const BAR_SLOTS = 5;

  // Playlists are created, renamed and deleted on the Playlists page now; old Manage links land where they went.
  const fromHash = () =>
    location.hash === '#/repairs' || location.hash === '#/manage/repairs'
      ? 'repairs'
      : location.hash === SETTINGS.href
        ? 'settings'
        : location.hash === INCOMING.href
          ? 'incoming'
          : location.hash.split('?')[0] === INSIGHTS.href
            ? 'insights'
            : 'playlists';
  let page = $state(fromHash());
  // What plays only matters where it is acted on: Playlists (add it) and Incoming (play an album).
  const withPlayer = $derived(page === 'playlists' || page === 'incoming');
  // The full-screen player goes back to its normal size whenever the navigation is used (also on the page already
  // shown) or the page changes; while it is full, the page behind it does not scroll.
  const collapse = () => (player.full = false);
  $effect(() => {
    page;
    collapse();
  });
  $effect(() => {
    document.documentElement.style.overflow = player.full && withPlayer ? 'hidden' : '';
  });
  // Same breakpoint as the CSS: from 1024 px the sidebar shows; below it the bottom bar.
  const wide = window.matchMedia('(min-width: 1024px)');
  let phone = $state(!wide.matches);
  $effect(() => {
    const on = () => (phone = !wide.matches);
    wide.addEventListener('change', on);
    return () => wide.removeEventListener('change', on);
  });
  let moreOpen = $state(false);

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

  // Connection line in the sidebar header.
  let status = $state<Status | null>(null);
  let statusFailed = $state(false);
  $effect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        status = await getStatus();
        statusFailed = false;
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
          ? { ok: true, text: `Connected to ${status.server_name ?? 'Jellyfin'}` }
          : { ok: false, text: 'Jellyfin unreachable' },
  );
  const MAIN = $derived<Page[]>(status?.incoming ? [INCOMING, PLAYLISTS, INSIGHTS, REPAIRS] : [PLAYLISTS, INSIGHTS, REPAIRS]);
  const ALL = $derived([...MAIN, SETTINGS]);
  const bar = $derived(ALL.length <= BAR_SLOTS ? ALL : MAIN.slice(0, BAR_SLOTS - 1));
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
      <a class="conn" class:down={!connection.ok} href="#/settings"><i aria-hidden="true"></i>{connection.text}</a>
    {/if}
    {#if !phone}
      <div class="side-picker" class:shown={known.list.length > 1}><SourcePicker /></div>
    {/if}

    <nav aria-label="Pages">
      {#each MAIN as p (p.id)}
        <a href={p.href} aria-current={page === p.id ? 'page' : undefined}><Icon name={p.icon} />{p.label}</a>
      {/each}
    </nav>

    <div class="foot">
      <a href={SETTINGS.href} aria-current={page === SETTINGS.id ? 'page' : undefined}><Icon name={SETTINGS.icon} />{SETTINGS.label}</a>
      {#if status}<div class="version">Cratarr {status.version}</div>{/if}
    </div>
  </aside>

  <div class="content">
    {#if withPlayer}
      {#if phone}<div class="top-picker" class:shown={known.list.length > 1}><SourcePicker /></div>{/if}
      <div class="np-bar" class:full={player.full}><NowPlaying /></div>
    {/if}
    {#if page === 'settings'}
      <Settings />
    {:else if page === 'incoming' && status?.incoming}
      <Incoming />
    {:else if status && !status.music}
      <p class="no-music">No Music library is set. Choose one in Settings → Libraries.</p>
    {:else if page === 'repairs'}
      <Repairs />
    {:else if page === 'insights'}
      <Insights />
    {:else}
      <Playlists />
    {/if}
  </div>

  {#if moreOpen}
    <button class="scrim" aria-label="Close menu" onclick={() => (moreOpen = false)}></button>
    <div class="sheet" id="more-sheet" role="dialog" aria-label="More pages" onclickcapture={collapse}>
      {#each overflow as p (p.id)}
        <a href={p.href} aria-current={page === p.id ? 'page' : undefined}><Icon name={p.icon} />{p.label}</a>
      {/each}
    </div>
  {/if}

  <nav class="bar" aria-label="Pages" onclickcapture={collapse}>
    {#each bar as p (p.id)}
      <a href={p.href} aria-current={page === p.id ? 'page' : undefined}><Icon name={p.icon} /><span>{p.label}</span></a>
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

  .content {
    min-width: 0;
    padding-bottom: calc(72px + env(safe-area-inset-bottom));
  }

  /* Same place and look as a page's own empty state. */
  .no-music {
    max-width: 900px;
    margin: 16px 0;
    padding: 0 clamp(12px, 2vw, 32px);
    color: var(--muted);
    font-size: 0.9rem;
  }

  /* The now-playing header stays on top of every page while it scrolls; same side padding as the pages. */
  .np-bar {
    position: sticky;
    top: 0;
    z-index: 3;
    padding: 0 clamp(12px, 2vw, 32px);
    background: var(--bg);
  }

  /* Full screen: the content area, the navigation left visible (the bottom bar here, the sidebar from 1024 px). */
  .np-bar.full {
    position: fixed;
    inset: 0;
    z-index: 15;
    display: flex;
    flex-direction: column;
    /* Runs on under the bottom bar (which stays on top), so no strip of the page shows between them. */
    padding-bottom: calc(72px + env(safe-area-inset-bottom));
    overflow-y: auto;
  }

  .bar {
    position: fixed;
    inset: auto 0 0;
    z-index: 20;
    display: flex;
    justify-content: space-around;
    padding: 6px 8px calc(6px + env(safe-area-inset-bottom));
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    backdrop-filter: blur(12px);
    border-top: 1px solid var(--border);
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
    inset: auto 8px calc(72px + env(safe-area-inset-bottom));
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

    .content {
      padding-bottom: 0;
    }

    .np-bar.full {
      inset: 0 0 0 240px;
      padding-bottom: 0;
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
