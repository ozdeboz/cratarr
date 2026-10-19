<script lang="ts">
  // Settings (decided 2026-10-16, as rows 2026-10-19): a list of the settings' parts in two groups, each user's own
  // (Personal) and, for the server's administrators only, the server's (decision 0051). A part is a row: its icon, its
  // name, what it is set to now in a line, and ›. A tap opens the part in the panel (lib/SettingsPanel): beside the list
  // on a roomy window, where a part is always open (App: the one last open), its row tinted; over the list on a phone,
  // as a playlist opens. About is a sheet from the info button at the page's top right (App, lib/AboutSheet).
  import { getImport, getLogLevels, getSettings, getSubsonicAccounts, getUsers, type ImportJob, type Settings } from './lib/api';
  import { me } from './lib/me.svelte';
  import { appInfo, downloads, inApp } from './lib/app.svelte';
  import Icon from './lib/Icon.svelte';
  import { panel, SETTINGS, settingsOf, viewPlaylist } from './lib/panel.svelte';
  import { playOn } from './lib/playOn.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { kept } from './lib/kept.svelte';
  import { logDetail, SECTIONS, settingsRead, settingsSaved, type SettingsSection } from './lib/settingsSections.svelte';

  // The last answer (kept for the panel too), read again here.
  const s = $derived<Settings | null>(settingsRead.s);
  let lastImport = $state<ImportJob | null>(null);
  let accounts = $state<string[] | null>(null);
  let users = $state<number | null>(null);
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
    getSubsonicAccounts().then(
      (a) => (accounts = a.accounts),
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
        if (!inApp) return prefs.browserName || 'This browser';
        return [appInfo.player, appInfo.stayReady ? 'Stays ready' : null, downloads.known ? `${size(downloads.bytes)} downloaded` : null].filter(Boolean).join(' · ');
      case 'connections':
        if (!s) return '';
        return [...s.sources.filter((v) => v.enabled).map((v) => (v.status.connected ? v.label : `${v.label} (not connected)`)), s.lidarr ? 'Lidarr' : null]
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
        return n === 0 ? 'None yet' : n === 1 ? '1 player' : `${n} players`;
      }
      case 'account':
        return [me.name, me.admin ? 'Administrator' : null].filter(Boolean).join(' · ');
      case 'users':
        return users === null ? '' : users === 1 ? '1 user' : `${users} users`;
      case 'auth':
        return accounts === null ? '' : accounts.length ? accounts.join(' · ') : 'No account';
      case 'import':
        return lastImport?.finished ? `Last ${new Date(lastImport.finished * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}` : 'Not yet';
      case 'network':
        return appInfo.inUse ? appInfo.inUse.replace(/^\w+:\/\//, '') : '';
      case 'sync':
        return [kept.known ? `${kept.items.length} kept` : null, appInfo.syncBudgetGb ? `${appInfo.syncBudgetGb} GB` : 'No limit'].filter(Boolean).join(' · ');
      case 'display':
        return THEME[prefs.theme];
      case 'diagnostics':
        return logDetail.on === null ? '' : logDetail.on ? 'More detail on' : 'More detail off';
    }
  }

  const shown = $derived(SECTIONS.filter((x) => (!x.app || inApp) && (!x.admin || me.admin)));
  const groups = $derived([shown.filter((x) => !x.admin), shown.filter((x) => x.admin)]);
</script>

<main>
  {#each groups as group, g (g)}
    {#if group.length}
      <section class="group" aria-labelledby="settings-g{g}">
        <h2 id="settings-g{g}">{g ? 'Server' : 'Personal'}</h2>
        <div class="list">
          {#each group as c (c.id)}
            {@const open = settingsOf(panel.viewed) === c.id}
            {@const l = line(c.id)}
            <button class="row" class:open aria-pressed={open} onclick={() => viewPlaylist(SETTINGS + c.id)}>
              <Icon name={c.icon} size={24} />
              <span class="text">
                <span class="name">{c.title}</span>
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
