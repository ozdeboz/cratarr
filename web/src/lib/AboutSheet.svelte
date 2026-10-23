<script lang="ts">
  // About Cratarr (the owner, 2026-10-16): a sheet from the info button at the settings page's top right, in place of an
  // About card among the settings. The version, since when it runs, where the playlists and the settings are, the port;
  // to anyone but an administrator, the version and since when (decision 0051).
  import { getAbout, getSettings, type Settings } from './api';
  import { me } from './me.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { since } from './settings/editor.svelte';
  import { settingsRead } from './settingsSections.svelte';

  let { onclose }: { onclose: () => void } = $props();

  // The last answer at once (the settings page's), then the server's latest.
  const s = $derived<Settings | null>(me.admin ? settingsRead.s : null);
  let about = $state<{ version: string; started: number } | null>(null);
  $effect(() => {
    if (me.admin)
      getSettings().then(
        (v) => (settingsRead.s = v),
        () => {},
      );
    else
      getAbout().then(
        (a) => (about = a),
        () => {},
      );
  });
</script>

<MenuSheet label="About" title="Cratarr" icon="info" chips={s ? [s.version] : about ? [about.version] : []} {onclose}>
  {#if about && !s}
    <dl>
      <dt>Running since</dt>
      <dd>{since(about.started)}</dd>
    </dl>
  {:else if s}
    <dl>
      <dt>Running since</dt>
      <dd>{since(s.started)}</dd>
      {#if s.music_folder}
        <dt>Playlists</dt>
        <dd>{s.playlists} in <code>{s.music_folder}</code></dd>
      {/if}
      <dt>Port</dt>
      <dd><code>{s.fixed.bind}</code></dd>
      <dt>Settings file</dt>
      <dd>
        {#if s.settings_file}<code>{s.settings_file}</code>{:else}<span class="bad">None, so settings can't be saved. Set CRATARR_DATA_DIR.</span>{/if}
      </dd>
    </dl>
  {/if}
</MenuSheet>

<style>
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 10px 16px;
    /* Padding, not margin: the sheet measures its content's height, which a child's margin would fall outside of. */
    margin: 0;
    padding: 4px 24px 20px;
    font-size: 15px;
  }

  dt {
    color: var(--muted);
  }

  dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  code {
    font-size: 14px;
  }

  .bad {
    color: var(--danger-ink);
  }
</style>
