<script lang="ts">
  // Settings (decided 2026-10-16): a portal of cards, one per part of the settings, each saying in a line what it is set
  // to now. A tap opens the part in the panel (lib/SettingsPanel), as a playlist opens: beside the cards on a wide page,
  // over them on a phone. The card whose part is open is bordered. This device's card only in the app. About is a sheet
  // from the info button at the page's top right (App, lib/AboutSheet). Each user's own parts first; the server's, under
  // its heading, for its administrators only (decision 0051).
  import { getImport, getSettings, getSubsonicAccounts, getUsers, type ImportJob, type Settings } from './lib/api';
  import { me } from './lib/me.svelte';
  import { appInfo, downloads, inApp } from './lib/app.svelte';
  import Icon from './lib/Icon.svelte';
  import { panel, SETTINGS, viewPlaylist } from './lib/panel.svelte';
  import { playOn } from './lib/playOn.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { kept } from './lib/kept.svelte';
  import { SECTIONS, settingsRead, settingsSaved, type SettingsSection } from './lib/settingsSections.svelte';

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
  });

  const size = (bytes: number) => (bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`);
  const THEME = { auto: 'System theme', light: 'Light', dark: 'Dark' } as const;

  /** What a part is set to now, in a line. */
  function line(id: SettingsSection): string {
    switch (id) {
      case 'device':
        if (!inApp) return 'This browser';
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
      case 'players':
        return playOn.players.map((p) => p.name).join(' · ') || 'None yet';
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
        return '';
    }
  }

  const shown = $derived(SECTIONS.filter((x) => (!x.app || inApp) && (!x.admin || me.admin)));
  const groups = $derived([shown.filter((x) => !x.admin), shown.filter((x) => x.admin)]);
</script>

<main>
  {#each groups as group, g (g)}
    {#if group.length}
      {#if g}<h2>Server</h2>{/if}
      <div class="cards">
        {#each group as c (c.id)}
          {@const open = panel.viewed === SETTINGS + c.id}
          <button class="card" class:open aria-pressed={open} onclick={() => viewPlaylist(SETTINGS + c.id)}>
            <Icon name={c.icon} size={28} />
            <span class="text">
              <strong>{c.id === 'device' ? ((inApp ? appInfo.player : prefs.browserName) || (inApp ? c.title : 'This browser')) : c.title}</strong>
              <small>{c.id === 'device' && inApp && appInfo.player ? c.title : line(c.id)}</small>
            </span>
          </button>
        {/each}
      </div>
    {/if}
  {/each}
</main>

<style>
  main {
    padding: 16px clamp(12px, 2vw, 32px) 96px;
  }

  h2 {
    margin: 28px 0 12px;
    font-size: 1.05rem;
    font-weight: 600;
  }

  /* As many columns as fit, each card at least 280 px: one on a phone. */
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
    max-width: 1200px;
  }

  .card {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 84px;
    padding: 14px 18px;
    border: 2px solid transparent;
    border-radius: 20px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .card:hover,
  .card:focus-visible {
    background: color-mix(in srgb, var(--text) 13%, transparent);
  }

  .card.open {
    border-color: var(--accent);
  }

  .card :global(svg) {
    flex: none;
    color: var(--accent);
  }

  .text {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  .text strong {
    font-size: 17px;
    font-weight: 600;
  }

  .text small {
    overflow: hidden;
    color: var(--muted);
    font-size: 14px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
