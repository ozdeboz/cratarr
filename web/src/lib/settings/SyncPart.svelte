<script lang="ts">
  // Sync (decision 0038): how this device keeps music: the storage budget, over which network, only while charging,
  // and in which quality. This device's own; it acts at once. Only in the app.
  import Seg from '../Seg.svelte';
  import SettingRow from '../SettingRow.svelte';
  import Switch from '../Switch.svelte';
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import { kept } from '../kept.svelte';
  import { downloadedSize } from './editor.svelte';

  const BUDGETS: { value: string; label: string }[] = [
    { value: '20', label: '20 GB' },
    { value: '50', label: '50 GB' },
    { value: '100', label: '100 GB' },
    { value: '0', label: 'No limit' },
  ];
  const QUALITY: { value: 'original' | 'smaller'; label: string }[] = [
    { value: 'original', label: 'Originals' },
    { value: 'smaller', label: 'Smaller copies' },
  ];
</script>

{#if inApp}
  <section aria-labelledby="sync-h">
    <h2 id="sync-h">Sync</h2>
    <p class="lede">This device only.</p>

    <SettingRow label="Storage" hint={kept.known ? `${downloadedSize(kept.used)} used` : null}>
      <Seg
        label="Storage"
        options={BUDGETS}
        bind:value={() => String(appInfo.syncBudgetGb), (v) => setDevice({ syncBudgetGb: Number(v) })}
      />
    </SettingRow>
    <SettingRow label="Quality">
      <Seg label="Quality" options={QUALITY} bind:value={() => appInfo.downloadQuality, (v) => setDevice({ downloadQuality: v })} />
    </SettingRow>
    <div class="pref">
      <Switch label="Download over mobile data" checked={appInfo.downloadOverMobile} onchange={(on) => setDevice({ downloadOverMobile: on })} />
    </div>
    <div class="pref">
      <Switch label="Only while charging" checked={appInfo.syncOnlyCharging} onchange={(on) => setDevice({ syncOnlyCharging: on })} />
    </div>
  </section>
{/if}
