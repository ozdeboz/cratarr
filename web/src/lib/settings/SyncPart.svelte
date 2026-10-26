<script lang="ts">
  // Sync (decision 0038): how this device keeps music: the storage budget, over which network, only while charging,
  // and in which quality. This device's own; it acts at once. Only in the app. Its Reset: all four back to their
  // defaults (lib/settings/reset); what is kept stays.
  import Seg from '../Seg.svelte';
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import { kept } from '../kept.svelte';
  import { downloadedSize } from './editor.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { offerReset } from './reset.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import { installed, keptBytes, offline } from '../offline.svelte';
  import { prefs } from '../prefs.svelte';

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

  offerReset(
    () => (inApp ? 'Storage, quality, mobile data and charging back to their defaults. What is kept stays.' : null),
    () => setDevice({ syncBudgetGb: 20, downloadQuality: 'original', downloadOverMobile: false, syncOnlyCharging: false }),
  );
  let choosingStorage = $state(false);
</script>

{#if inApp}
  <Group>
    <!-- Four choices: the row says it, the choice sheet picks it (the owner, 2026-10-25). -->
    <Item
      name="Storage"
      line={kept.known ? `${downloadedSize(kept.used)} used` : null}
      more="When it runs out, what was kept longest ago shrinks first: its tracks furthest from its start go."
      value={BUDGETS.find((b) => b.value === String(appInfo.syncBudgetGb))?.label ?? null}
      onclick={() => (choosingStorage = true)}
    />
    <Item name="Quality" more="Originals: the files as they are. Smaller copies: Cratarr makes 160 kb/s Opus copies of lossless files; lossy files come as they are.">
      <Seg label="Quality" options={QUALITY} bind:value={() => appInfo.downloadQuality, (v) => setDevice({ downloadQuality: v })} />
    </Item>
    <Item name="Download over mobile data" toggle={{ checked: appInfo.downloadOverMobile, onchange: (on) => setDevice({ downloadOverMobile: on }) }} />
    <Item name="Only while charging" toggle={{ checked: appInfo.syncOnlyCharging, onchange: (on) => setDevice({ syncOnlyCharging: on }) }} />
  </Group>
{/if}
{#if !inApp && installed}
  <!-- The installed page's own downloads (0.9.8, lib/offline): what it holds, and in which quality (the owner,
       2026-10-26: Opus in the options too). -->
  <Group>
    <Item name="Kept in this browser" value={`${Object.keys(offline.kept).length} tracks · ${downloadedSize(keptBytes())}`} />
    <Item name="Quality" more="Smaller copies: Cratarr makes Opus copies of lossless files (AAC where this browser plays no Opus), about a fifth of the size; lossy files come as they are. Originals: the files as they are, where this browser plays them.">
      <Seg label="Quality" options={QUALITY} bind:value={() => prefs.offlineQuality, (v) => (prefs.offlineQuality = v)} />
    </Item>
  </Group>
{/if}
{#if choosingStorage}
  <ChoiceSheet title="Storage" options={BUDGETS} value={String(appInfo.syncBudgetGb)} onchange={(v) => setDevice({ syncBudgetGb: Number(v) })} onclose={() => (choosingStorage = false)} />
{/if}
