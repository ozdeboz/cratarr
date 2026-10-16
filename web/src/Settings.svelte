<script lang="ts">
  // Settings (decided 2026-10-16): a portal of cards, one per part of the settings, each saying in a line what it is set
  // to now. A tap opens the part in the panel (lib/SettingsPanel), as a playlist opens: beside the cards on a wide page,
  // over them on a phone. The card whose part is open is bordered. This device's card only in the app.
  import { getSettings, getSubsonicAccounts, type Settings } from './lib/api';
  import { appInfo, downloads, inApp } from './lib/app.svelte';
  import Icon from './lib/Icon.svelte';
  import { panel, SETTINGS, viewPlaylist } from './lib/panel.svelte';
  import { playOn } from './lib/playOn.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { SECTIONS, settingsRead, settingsSaved, type SettingsSection } from './lib/settingsSections.svelte';

  // The last answer (kept for the panel too), read again here.
  const s = $derived<Settings | null>(settingsRead.s);
  let accounts = $state<string[] | null>(null);
  // Read again after each save in the panel.
  $effect(() => {
    void settingsSaved.n;
    getSettings().then(
      (v) => (settingsRead.s = v),
      () => {},
    );
    getSubsonicAccounts().then(
      (a) => (accounts = a.accounts),
      () => {},
    );
  });

  const size = (bytes: number) => (bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`);
  const THEME = { auto: 'System theme', light: 'Light', dark: 'Dark' } as const;

  /** What a part is set to now, in a line. */
  function line(id: SettingsSection): string {
    switch (id) {
      case 'device':
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
      case 'subsonic':
        return accounts === null ? '' : accounts.length ? accounts.join(' · ') : 'No account';
      case 'display':
        return THEME[prefs.theme];
      case 'about':
        return s ? `Cratarr ${s.version}` : '';
    }
  }

  const shown = $derived(SECTIONS.filter((x) => !x.app || inApp));
</script>

<main>
  <div class="cards">
    {#each shown as c (c.id)}
      {@const open = panel.viewed === SETTINGS + c.id}
      <button class="card" class:open aria-pressed={open} onclick={() => viewPlaylist(SETTINGS + c.id)}>
        <Icon name={c.icon} size={28} />
        <span class="text">
          <strong>{c.id === 'device' ? (appInfo.player ?? c.title) : c.title}</strong>
          <small>{c.id === 'device' && appInfo.player ? c.title : line(c.id)}</small>
        </span>
      </button>
    {/each}
  </div>
</main>

<style>
  main {
    padding: 16px clamp(12px, 2vw, 32px) 96px;
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
