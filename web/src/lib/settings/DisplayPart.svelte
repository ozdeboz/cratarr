<script lang="ts">
  // Display: this browser's own look; acts at once.
  import Seg from '../Seg.svelte';
  import SettingRow from '../SettingRow.svelte';
  import Switch from '../Switch.svelte';
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import { prefs, type Theme } from '../prefs.svelte';

  const THEMES: { value: Theme; label: string }[] = [
    { value: 'auto', label: 'System' },
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' },
  ];
</script>

  <section aria-labelledby="display-h">
    <h2 id="display-h">Display</h2>
    <p class="lede">Remembered by this browser only.</p>

    <SettingRow label="Theme"><Seg label="Theme" options={THEMES} bind:value={prefs.theme} /></SettingRow>

    <div class="pref">
      <Switch label="Progress bar under the playing track" checked={prefs.showProgress} onchange={(on) => (prefs.showProgress = on)} />
    </div>
    <!-- Levelling (decision 0035): in the app its own player's, here this browser's. -->
    <div class="pref">
      <Switch
        label="Level the volume"
        hint="Every track about as loud as the next"
        checked={inApp ? appInfo.levelling : prefs.levelling}
        onchange={(on) => (inApp ? setDevice({ levelling: on }) : (prefs.levelling = on))}
      />
    </div>
  </section>
