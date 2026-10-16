<script lang="ts">
  // Display: this browser's own look; acts at once.
  import Switch from '../Switch.svelte';
  import { prefs, type Theme } from '../prefs.svelte';

  const THEMES: { id: Theme; label: string }[] = [
    { id: 'auto', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];
</script>

  <section aria-labelledby="display-h">
    <h2 id="display-h">Display</h2>
    <p class="lede">Remembered by this browser only.</p>

    <div class="pref">
      <span class="l" id="theme-l">Theme</span>
      <div class="seg" role="group" aria-labelledby="theme-l">
        {#each THEMES as t (t.id)}
          <button aria-pressed={prefs.theme === t.id} onclick={() => (prefs.theme = t.id)}>{t.label}</button>
        {/each}
      </div>
    </div>

    <div class="pref">
      <Switch label="Progress bar under the playing track" checked={prefs.showProgress} onchange={(on) => (prefs.showProgress = on)} />
    </div>
  </section>
