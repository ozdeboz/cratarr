<script lang="ts">
  // Download… (decision 0058, amended 0.10; the owner, 2026-10-30: "Download is not coherently present"): one entry
  // on everything that has tracks, one step in its menu sheet. In the app first where: to this device (offline in the
  // app, lib/DownloadEntry: the row fills as they come in) or as files (lib/ExportStep: the format, the layout, one zip
  // or the files one by one); in an installed page to this browser or as files; in the browser files only. Where it
  // went last is kept with the export's choices (lib/export.svelte). Never Bandcamp's previews (decision 0071): their
  // menus have no Download….
  import type { ExportWhat } from './api';
  import { inApp, type Downloadable } from './app.svelte';
  import DownloadEntry from './DownloadEntry.svelte';
  import { exporting } from './export.svelte';
  import ExportStep from './ExportStep.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { installed } from './offline.svelte';
  import Seg from './Seg.svelte';

  let {
    what,
    tracks = null,
    onclose,
  }: {
    /** As files; null: none (nothing the server can export). */
    what: ExportWhat | null;
    /** To this device: its tracks as the app keeps them; null while not known, or where it cannot (the browser). */
    tracks?: Downloadable[] | null;
    onclose: () => void;
  } = $props();

  const here = inApp || installed;
  const toDevice = $derived(here && !!tracks?.length);
  const where = $derived(!toDevice ? 'files' : !what ? 'device' : exporting.where);
</script>

{#if toDevice && what}
  <div class="where">
    <Seg
      label="Where"
      options={[
        { value: 'device', label: inApp ? 'To this device' : 'In this browser' },
        { value: 'files', label: 'As files' },
      ]}
      bind:value={exporting.where}
    />
  </div>
{/if}
{#if where === 'device' && tracks}
  <MenuCards>
    <MenuGroup><DownloadEntry {tracks} /></MenuGroup>
  </MenuCards>
{:else if what}
  <ExportStep {what} {onclose} />
{/if}

<style>
  .where {
    display: grid;
    padding: 4px 16px 8px;
  }
</style>
