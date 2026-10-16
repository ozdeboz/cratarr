<script lang="ts">
  // About: the version, the port, where the files are.
  import { ed } from './editor.svelte';
  import { since } from './editor.svelte';

  // Shown only once the settings are read (lib/SettingsPanel).
  const s = $derived(ed.s!);
</script>

  <section aria-labelledby="about-h">
    <h2 id="about-h">About</h2>
    <dl>
      <dt>Cratarr</dt>
      <dd>{s.version}, running since {since(s.started)}</dd>
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
  </section>
