<script lang="ts">
  // Display: this browser's own look; acts at once. Its Reset: the theme, the progress bar and levelling back to their
  // defaults (lib/settings/reset).
  import Seg from '../Seg.svelte';
  import SettingRow from '../SettingRow.svelte';
  import Switch from '../Switch.svelte';
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import { setLevelling } from '../localPlayer.svelte';
  import { prefs, type Theme } from '../prefs.svelte';
  import { offerReset } from './reset.svelte';

  const THEMES: { value: Theme; label: string }[] = [
    { value: 'auto', label: 'System' },
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' },
  ];

  const setLevel = (on: boolean) => (inApp ? setDevice({ levelling: on }) : setLevelling(on));
  offerReset(
    () => 'Theme, progress bar and levelling back to their defaults.',
    () => {
      prefs.theme = 'auto';
      prefs.showProgress = true;
      setLevel(true);
    },
  );
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
        more={"Each track plays at about the same loudness, as Cratarr measured it (or as the file's ReplayGain tags say, until it has).\n\nAn album played through keeps its own dynamics; shuffle and radio are levelled track by track."}
        checked={inApp ? appInfo.levelling : prefs.levelling}
        onchange={setLevel}
      />
    </div>
  </section>
