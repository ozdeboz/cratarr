<script lang="ts">
  // Settings (decided 2026-10-16, as rows 2026-10-19): a list of the settings' parts in two groups, each user's own
  // (Personal) and, for the server's administrators only, the server's (decision 0051). A part is a row: its icon, its
  // name, what it is set to now in a line, and ›. A tap opens the part in the panel (lib/SettingsPanel): beside the list
  // on a roomy window, where a part is always open (App: the one last open), its row tinted; over the list on a phone,
  // as a playlist opens. About is a sheet from the info button, the pill's own (App, lib/AboutSheet).
  import { deviceName } from './lib/playOnTarget.svelte';
  import { getHomeAssistant, getImport, getLogLevels, getSettings, getSignInRequired, getUsers, type ImportJob, type Settings } from './lib/api';
  import { me } from './lib/me.svelte';
  import { appInfo, downloads, inApp } from './lib/app.svelte';
  import Icon from './lib/Icon.svelte';
  import { panel, SETTINGS, settingsOf, viewPlaylist } from './lib/panel.svelte';
  import { playOn } from './lib/playOn.svelte';
  import { HOME_PARTS, prefs } from './lib/prefs.svelte';
  import { HOME_HEADER, localCovers } from './lib/localCovers.svelte';
  import { kept } from './lib/kept.svelte';
  import { crates, loadCrates } from './lib/crates.svelte';
  import { excluded, loadExcluded } from './lib/exclusions.svelte';
  import { logDetail, SECTIONS, settingsRead, settingsSaved, type SettingsSection } from './lib/settingsSections.svelte';
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
    getLogLevels().then(
      (l) => (logDetail.on = Object.keys(l.now).length > 0),
      () => {},
    );
  });

  const size = (bytes: number) => (bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`);
  const THEME = { auto: 'System theme', light: 'Light', dark: 'Dark' } as const;

  /** What a part is set to now, in a line. */
  function line(id: SettingsSection): string {
    switch (id) {
      case 'device':
        // Its name is the row's title; the line says what it is (the owner, 2026-10-25).
        if (!inApp) return 'This browser';
        return ['This device', appInfo.stayReady ? 'Stays ready' : null, downloads.known ? `${size(downloads.bytes)} downloaded` : null].filter(Boolean).join(' · ');
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
        // Counted, as Users: their names do not fit a line.
        const n = playOn.players.length;
        if (!playOn.known) return '';
        return n === 0 ? 'None yet' : n === 1 ? '1 player' : `${n} players`;
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
      case 'import':
        if (lastImport === undefined) return '';
        return lastImport?.finished ? `Last ${new Date(lastImport.finished * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}` : 'Not yet';
      case 'network':
        return appInfo.inUse ? appInfo.inUse.replace(/^\w+:\/\//, '') : '';
      case 'sync':
        return [kept.known ? `${kept.items.length} kept` : null, appInfo.syncBudgetGb ? `${appInfo.syncBudgetGb} GB` : 'No limit'].filter(Boolean).join(' · ');
      case 'appearance':
        return [THEME[prefs.theme], prefs.animations ? null : 'No animations', prefs.homeGreeting ? 'Greeting' : null, localCovers[HOME_HEADER] ? 'Your picture' : null].filter(Boolean).join(' · ');
      case 'layout':
        return `Bar along the ${prefs.navAt} · Mini player along the ${prefs.miniAt}`;
      case 'player':
        return prefs.playerRow ? (prefs.playerFacts.length ? `${prefs.playerFacts.length} extra details` : 'Extra details') : 'No extra details';
      case 'content': {
        const n = prefs.homeParts.length;
        return [n === HOME_PARTS.length ? 'Every Home section' : `${n} of ${HOME_PARTS.length} Home sections`, prefs.profile ? null : 'No music profiles']
          .filter(Boolean)
          .join(' · ');
      }
      case 'taste': {
        const crate = (crates.list ?? []).filter((c) => (c.left_out ?? 0) > 0).length;
        return [excluded.n ? `${excluded.n.toLocaleString()} excluded everywhere` : null, crate ? `${crate} ${crate === 1 ? 'crate' : 'crates'}` : null]
          .filter(Boolean)
          .join(' · ') || 'Nothing excluded';
      }
      case 'diagnostics':
        return logDetail.on === null ? '' : logDetail.on ? 'More detail on' : 'More detail off';
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
  const allowed = $derived(SECTIONS.filter((x) => (!x.app || inApp) && (!x.admin || me.admin || x.everyone)));
  const shown = $derived(
    words.length
      ? allowed.filter((x) => {
          const f = found(x.id, x.title)!;
          return f.title || f.rows.length > 0;
        })
      : allowed,
  );
  // A part of the server's that anyone opens (Connections) is Personal for someone not an administrator.
  const groups = $derived([shown.filter((x) => !x.admin || !me.admin), shown.filter((x) => x.admin && me.admin)]);
</script>

<main>
  {#if words.length && !shown.length}<p class="none">No settings match.</p>{/if}
  {#each groups as group, g (g)}
    {#if group.length}
      <section class="group" aria-labelledby="settings-g{g}">
        <h2 id="settings-g{g}">{g ? 'Server' : 'Personal'}</h2>
        <div class="list">
          {#each group as c (c.id)}
            {@const open = settingsOf(panel.viewed) === c.id}
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
</style>
