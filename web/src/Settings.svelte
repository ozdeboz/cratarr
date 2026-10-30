<script lang="ts">
  // Settings (decided 2026-10-16, as rows 2026-10-19): a list of the settings' parts in two groups, each user's own
  // (Personal) and, for the server's administrators only, the server's (decision 0051). A part is a row: its icon, its
  // name, what it is set to now in a line, and ›. A tap opens the part in the panel (lib/SettingsPanel): beside the list
  // on a roomy window, where a part is always open (App: the one last open), its row tinted; over the list on a phone,
  // as a playlist opens. About is a sheet from the info button, the pill's own (App, lib/AboutSheet).
  import { installed } from './lib/offline.svelte';
  import { panelBeside } from './lib/glass.svelte';
  import { library } from './lib/albums.svelte';
  import { serverConnection } from './lib/connection.svelte';
  import { switchable } from './lib/deviceMusic.svelte';
  import ModeSwitcher from './lib/ModeSwitcher.svelte';
  import { AUTOPLAY_NAMES, RADIO_NAMES } from './lib/MoreMusicSheet.svelte';
  import { defaults, mine } from './lib/mySettings.svelte';
  import { OWN } from './lib/sources.svelte';
  import { plural } from './lib/insights.svelte';
  import { deviceName } from './lib/playOnTarget.svelte';
  import { getHomeAssistant, getImport, getLog, getSettings, getSignInRequired, getUsers, type ImportJob, type Settings } from './lib/api';
  import { me } from './lib/me.svelte';
  import { appInfo, inApp } from './lib/app.svelte';
  import { levelOn } from './lib/level.svelte';
  import Icon from './lib/Icon.svelte';
  import { panel, SETTINGS, settingsOf, viewPlaylist } from './lib/panel.svelte';
  import { playOn } from './lib/playOn.svelte';
  import { HOME_PARTS, homeDefault, prefs } from './lib/prefs.svelte';
  import { HOME_HEADER, localCovers } from './lib/localCovers.svelte';
  import { kept } from './lib/kept.svelte';
  import { crates, loadCrates } from './lib/crates.svelte';
  import { excluded, loadExcluded } from './lib/exclusions.svelte';
  import { SECTIONS, settingsRead, settingsSaved, type SettingsSection } from './lib/settingsSections.svelte';
  import { pageView, usePageAction } from './lib/pageView.svelte';
  import { about } from './lib/AboutSheet.svelte';
  import { loadSettingsWords, settingsMatches } from './lib/settingsSearch.svelte';
  import { words as searchWords } from './lib/search';

  // The last answer (kept for the panel too), read again here.
  const s = $derived<Settings | null>(settingsRead.s);
  /** undefined until read: no "Not yet" before the answer. */
  let lastImport = $state<ImportJob | null | undefined>(undefined);
  let signIn = $state<boolean | null>(null);
  let homeAssistant = $state(false);
  let users = $state<number | null>(null);
  // Musical taste's line: what is excluded, everywhere and from crates.
  $effect(() => {
    loadExcluded();
    void loadCrates();
  });
  // Read again after each save in the panel; the server's settings only for its administrators.
  $effect(() => {
    void settingsSaved.n;
    if (!me.admin) return;
    getImport().then(
      (r) => (lastImport = r.job),
      () => {},
    );
    getSettings().then(
      (v) => (settingsRead.s = v),
      () => {},
    );
    getSignInRequired().then(
      (r) => (signIn = r.required),
      () => {},
    );
    getHomeAssistant().then(
      (r) => (homeAssistant = r.keys.length > 0),
      () => {},
    );
    getUsers().then(
      (u) => (users = u.length),
      () => {},
    );
    // Diagnostics' line: only the most important (the owner, 2026-10-25): errors in the last day.
    getLog({ level: 'error', limit: 500 }).then(
      (r) => (errorsToday = r.rows.filter((x) => x.last > Date.now() - 86_400_000).reduce((n, x) => n + Math.max(1, x.count), 0)),
      () => {},
    );
  });
  let errorsToday = $state<number | null>(null);

  const THEME = { auto: 'System theme', light: 'Light', dark: 'Dark' } as const;

  /** A preference part's line: what differs from its default, or "Default" (the owner, 2026-10-25). */
  const changed = (what: (string | false | null | undefined)[]) => what.filter(Boolean).join(' · ') || 'Default';
  /** What a part is set to now, in a line. */
  function line(id: SettingsSection): string {
    switch (id) {
      case 'device':
        // Its name is the row's title; its line what of its own differs from the default, as every part of preferences
        // (the owner, 2026-10-25: what is downloaded is Sync's).
        return changed([
          !levelOn() && 'No levelling',
          !inApp && prefs.phoneLossless && 'Lossless',
          inApp && appInfo.stayReady && 'Stays ready',
          inApp && !appInfo.batteryAllowed && 'Android may stop it',
          inApp && appInfo.deviceMusic === 'granted' && 'Its own music',
        ]);
      case 'connections':
        if (!s) return '';
        return [...s.sources.filter((v) => v.enabled).map((v) => (v.status.connected ? v.label : `${v.label} (not connected)`)), s.lidarr ? 'Lidarr' : null, homeAssistant ? 'Home Assistant' : null]
          .filter(Boolean)
          .join(' · ') || 'No server';
      case 'libraries': {
        if (!s) return '';
        const names = s.sources.filter((v) => v.enabled).flatMap((v) =>
          [...v.libraries.music, ...v.libraries.incoming].map((id) => v.server_libraries.find((l) => l.id === id)?.name ?? id),
        );
        return names.join(' · ') || 'None in use';
      }
      case 'players': {
        // Counted by kind, as Users: their names do not fit a line (the owner, 2026-10-25: what is set up; a browser is a
        // player only while it is open).
        if (!playOn.known) return '';
        const apps = playOn.players.filter((p) => !p.kind || p.kind === 'app').length;
        const kodis = playOn.players.filter((p) => p.kind === 'kodi').length;
        return [apps ? plural(apps, 'app') : null, kodis ? `${kodis} Kodi` : null].filter(Boolean).join(' · ') || 'None yet';
      }
      case 'account':
        return [me.name, me.admin ? 'Administrator' : null].filter(Boolean).join(' · ');
      case 'users':
        return users === null ? '' : users === 1 ? '1 user' : `${users} users`;
      case 'auth':
        // Whether every request needs a sign-in (the accounts are under Users, 0051 amended; Home Assistant's key under Connections).
        return [signIn === null ? null : signIn ? 'Sign-in required' : 'Sign-in not required']
          .filter(Boolean)
          .join(' · ');
      // One's own imports (ListenBrainz, Last.fm, files): their state is on their page.
      case 'import':
        return '';
      case 'serverimport':
        if (lastImport === undefined) return '';
        return lastImport?.finished ? `Last ${new Date(lastImport.finished * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}` : 'Not yet';
      case 'network':
        return appInfo.inUse ? appInfo.inUse.replace(/^\w+:\/\//, '') : '';
      case 'sync':
        return [kept.known ? `${kept.items.length} kept` : null, appInfo.syncBudgetGb ? `${appInfo.syncBudgetGb} GB` : 'No limit'].filter(Boolean).join(' · ');
      // A part of preferences says only what differs from how it came (the owner, 2026-10-25), else "Default".
      case 'appearance':
        return changed([
          prefs.theme !== 'auto' && THEME[prefs.theme],
          !prefs.animations && 'No animations',
          !prefs.homeGreeting && 'No greeting',
          !!localCovers[HOME_HEADER] && 'Your picture',
        ]);
      case 'layout':
        return changed([
          prefs.navAt === 'bottom' && 'Bar at the bottom',
          prefs.navAt === 'top' && prefs.navHides && 'Bar hides when scrolling',
          prefs.navAt === 'top' && !prefs.navHidesOnItem && 'Bar stays over library items',
          !prefs.stickyActions && 'Action bar scrolls away',
          prefs.miniAt === 'top' && 'Mini player at the top',
          prefs.navControlsAt === 'bottom' && 'Navigation controls at the bottom',
          prefs.contentControlsAt === 'bottom' && 'Content controls at the bottom',
        ]);
      case 'player':
        return changed([!prefs.playerRow && 'No extra details', prefs.playerRow && prefs.playerFacts.length > 0 && plural(prefs.playerFacts.length, 'extra detail')]);
      case 'content': {
        const n = prefs.homeParts.length;
        return changed([
          prefs.homeParts.join() !== homeDefault().join() && `${n} of ${HOME_PARTS.length} Home sections`,
          !prefs.profile && 'No music profiles',
        ]);
      }
      case 'taste': {
        const crate = (crates.list ?? []).filter((c) => (c.left_out ?? 0) > 0).length;
        return [excluded.n ? `${excluded.n.toLocaleString()} excluded everywhere` : null, crate ? `${crate} ${crate === 1 ? 'crate' : 'crates'}` : null]
          .filter(Boolean)
          .join(' · ') || 'Nothing excluded';
      }
      case 'radio': {
        const off = (library.view?.sources.find((s) => s.id === OWN)?.library_info ?? []).filter(
          (l) => mine.moreFrom.libraries !== null && !mine.moreFrom.libraries.includes(l.id),
        );
        const d = defaults;
        return changed([
          !d.autoplay.arm && 'No auto arm',
          d.autoplay.mode !== 'similar' && AUTOPLAY_NAMES[d.autoplay.mode],
          d.autoplay.albums && 'Queue albums',
          d.radio.by !== 'track' && `Radio: ${RADIO_NAMES[d.radio.by]}`,
          d.radio.count !== 5 && `${d.radio.count} after each track`,
          d.skipFavourites && 'Ignore favourites',
          ...off.map((l) => `Not ${l.name}`),
          mine.moreFrom.bandcamp && 'Bandcamp',
        ]);
      }
      case 'tracking':
        return changed([!mine.tracking && 'No play history']);
      case 'diagnostics':
        return errorsToday === null ? '' : errorsToday ? plural(errorsToday, 'error') + ' today' : 'No errors today';
    }
  }

  // The pill's own button: About.
  usePageAction(() => ({ icon: 'info', label: 'About', run: () => (about.open = true) }));

  // The pill's search (lib/settingsSearch): the parts that hold what is searched, each saying where in its line.
  // As every search reads (lib/search, decision 0072).
  const words = $derived(searchWords(pageView.query));
  $effect(() => {
    if (words.length) void loadSettingsWords();
  });
  const found = (id: SettingsSection, title: string) => (words.length ? settingsMatches(id, title, words) : null);
  // Sync in the installed page too (0.9.8: its own downloads, lib/offline).
  const allowed = $derived(SECTIONS.filter((x) => (!x.app || inApp || (x.id === 'sync' && installed)) && (!x.admin || me.admin || x.everyone)));
  const shown = $derived(
    words.length
      ? allowed.filter((x) => {
          const f = found(x.id, x.title)!;
          return f.title || f.rows.length > 0;
        })
      : allowed,
  );
  // Personal, Device, Server (lib/settingsSections); a part of the server's that anyone opens (Connections) is Personal
  // for someone not an administrator.
  const keptOf = (x: (typeof SECTIONS)[number]) => (x.admin && !me.admin ? 'personal' : x.kept);
  const GROUPS = [
    { kept: 'personal', title: 'Personal' },
    { kept: 'device', title: 'Device' },
    { kept: 'server', title: 'Server' },
  ] as const;
  const groups = $derived(GROUPS.map((g) => ({ ...g, parts: shown.filter((x) => keptOf(x) === g.kept) })));
</script>

<main>
  <!-- The mode switch where the bar does not have it (This device › Mode switch in the bar, off by default; the owner,
       2026-10-25): the very top of Settings. -->
  {#if switchable() && !prefs.modeSwitchInBar && !words.length}<div class="mode"><ModeSwitcher connection={serverConnection.value} /></div>{/if}
  {#if words.length && !shown.length}<p class="none">No settings match.</p>{/if}
  {#each groups as group, g (g)}
    {#if group.parts.length}
      <section class="group" aria-labelledby="settings-g{g}">
        <h2 id="settings-g{g}">{group.title}</h2>
        <div class="list">
          {#each group.parts as c (c.id)}
            {@const open = panelBeside() && settingsOf(panel.viewed) === c.id}
            {@const f = found(c.id, c.title)}
            {@const l = f?.rows.length ? f.rows.slice(0, 4).join(' · ') : line(c.id)}
            <button class="row" class:open aria-pressed={open} onclick={() => viewPlaylist(SETTINGS + c.id)}>
              <Icon name={c.icon} size={24} />
              <span class="text">
                <span class="name">{c.id === 'device' ? deviceName() : c.title}</span>
                {#if l}<small>{l}</small>{/if}
              </span>
              <span class="chev"><Icon name="chevronRight" size={20} /></span>
            </button>
          {/each}
        </div>
      </section>
    {/if}
  {/each}
</main>

<style>
  main {
    padding: 16px clamp(12px, 2vw, 32px) 96px;
  }

  .none {
    margin: 8px 4px;
    color: var(--muted);
    font-size: 0.9rem;
  }

  /* One column, never wider than a phone held sideways: beside a part it is the list's own column (App). */
  .group {
    max-width: 640px;
  }

  .group + .group {
    margin-top: 24px;
  }

  h2 {
    margin: 0 4px 8px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }

  /* A group's rows in one rounded block, a line between them. */
  .list {
    overflow: hidden;
    border-radius: 14px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 64px;
    padding: 10px 14px 10px 16px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .row:hover,
  .row:focus-visible {
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  /* The part open beside the list. */
  .row.open {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .row > :global(svg) {
    flex: none;
    color: var(--accent);
  }

  .text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .name {
    font-size: 15px;
  }

  .text small {
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chev {
    display: grid;
    flex: none;
    color: var(--muted);
  }

  /* The mode switch at the top, as the groups' width. */
  .mode {
    margin-bottom: 4px;
  }
</style>
