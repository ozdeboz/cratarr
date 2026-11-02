<script lang="ts">
  // Download… as files (decision 0058, amended 0.10; lib/DownloadStep): one step for every object and selection,
  // inside its menu sheet (lib/MenuSheet), and the panel's Export tab under its playlists. For playlists their files
  // only or with their tracks, several as they are or combined into one; for a list of tracks with a playlist file or not; the format and
  // the layout; in the app, one zip or the files one by one; then Download, saying what it will hold (how many tracks, about how large; how many missing are left out under
  // it), which readies it on the server and starts
  // the download (a link: the browser shows its progress, and closing it cancels; the files one by one, the app writes
  // into a folder picked there). The choices are kept in this browser (lib/export.svelte).
  import { exportDownloadUrl, exportPreview, prepareExport, type ExportPreview, type ExportWhat } from './api';
  import { inApp, saveExport } from './app.svelte';
  import Card from './Card.svelte';
  import ChoiceList from './ChoiceList.svelte';
  import { exporting } from './export.svelte';
  import { BITRATES, exportLabel, exportLeftOut, exportReady, exportRequest, FORMATS, playlistFileOffered, several, suggestedName, withTracks } from './exportChoices';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { shownPlaylists } from './output.svelte';
  import Seg from './Seg.svelte';
  import SettingRow from './SettingRow.svelte';
  import Switch from './Switch.svelte';

  let { what, onclose }: { what: ExportWhat; onclose: () => void } = $props();

  // The combined playlist's name follows the playlists until it is typed in.
  const suggested = $derived.by(() => {
    if (what.kind !== 'playlists') return '';
    const all = shownPlaylists();
    return suggestedName(what.names.map((id) => all.find((p) => p.id === id)?.name ?? id.slice(id.indexOf('/') + 1)));
  });
  $effect(() => {
    const s = suggested;
    if (!exporting.named && s) exporting.name = s;
  });

  // The files one by one only where the app writes them.
  const req = $derived(exportRequest(what, exporting));
  const ready = $derived(exportReady(req));
  const tracks = $derived(withTracks(what, exporting));

  // What it will hold, asked for again 300 ms after the last change.
  let preview = $state<ExportPreview | null>(null);
  let failed = $state<string | null>(null);
  $effect(() => {
    const r = req;
    if (!ready) {
      preview = null;
      return;
    }
    let gone = false;
    const t = setTimeout(() => {
      exportPreview(r).then(
        (p) => {
          if (gone) return;
          preview = p;
          failed = null;
        },
        (e) => {
          if (gone) return;
          preview = null;
          failed = e instanceof Error ? e.message : 'Unexpected error';
        },
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });

  // Export: the server readies it, then the download starts as a link's would.
  let starting = $state(false);
  async function start() {
    starting = true;
    try {
      const { token, name } = await prepareExport(req);
      // In the app, saved where its user picks; in a browser, a link's download.
      if (inApp) {
        saveExport(token, name);
        onclose();
        return;
      }
      const a = document.createElement('a');
      a.href = exportDownloadUrl(token);
      a.download = name;
      document.body.append(a);
      a.click();
      a.remove();
      onclose();
    } catch (e) {
      failed = e instanceof Error ? e.message : 'Unexpected error';
    } finally {
      starting = false;
    }
  }
</script>

<!-- Playlist only first (the owner, 2026-11-02: off as it comes; on, what concerns the tracks folds away), then the format
     (the first and most important of the tracks'), the other switches, the layout last; then Download, as offline
     listening's: a row of the menu's, its words saying how much (no line above it). -->
<div class="parts">
  {#if what.kind === 'playlists'}
    <Switch card label="Playlist only" checked={exporting.playlistOnly} onchange={(on) => (exporting.playlistOnly = on)} />
  {/if}
  {#if tracks}
    <Card title="Format" label="Format">
      <ChoiceList label="Format" options={FORMATS} value={exporting.format} onchange={(v) => (exporting.format = v)} />
    </Card>
    {#if exporting.format === 'opus'}
      <SettingRow card label="Bit rate" hint="kb/s">
        <Seg
          label="Bit rate"
          options={BITRATES.map((n) => ({ value: String(n), label: String(n), title: `${n} kb/s` }))}
          bind:value={() => String(exporting.bitrate), (v) => (exporting.bitrate = Number(v))}
        />
      </SettingRow>
    {/if}
  {/if}

  {#if what.kind === 'playlists'}
    {#if several(what)}
      <Switch card label="Combine into one playlist" checked={exporting.combine} onchange={(on) => (exporting.combine = on)} />
      {#if exporting.combine}
        <input
          type="text"
          aria-label="Name of the combined playlist"
          maxlength="100"
          bind:value={exporting.name}
          oninput={() => (exporting.named = true)}
        />
      {/if}
    {/if}
  {:else if playlistFileOffered(what)}
    <Switch card label="Include a playlist file" checked={exporting.m3u} onchange={(on) => (exporting.m3u = on)} />
  {/if}
  {#if tracks}
    <!-- The layout as a switch (the owner): on, Artist and album folders; off, one folder. -->
    <Switch card label="Preserve directory layout" checked={exporting.layout === 'folders'} onchange={(on) => (exporting.layout = on ? 'folders' : 'flat')} />
  {/if}
  {#if failed}<p class="error" role="alert">{failed}</p>{/if}
</div>
<MenuCards>
  <MenuGroup>
    <MenuEntry
      icon="arrowDownToLine"
      label={exportLabel(preview, req)}
      sub={exportLeftOut(preview)}
      disabled={!ready || starting}
      busy={starting}
      onclick={start}
    />
  </MenuGroup>
</MenuCards>

<style>
  /* Each choice a row on a card of its own (lib/SettingRow, lib/Card, lib/Switch), as View's. */
  .parts {
    display: grid;
    gap: 4px;
    padding: 4px 16px 8px;
  }

  .error {
    margin: 8px 0 0;
    color: var(--danger-ink);
    font-size: 0.9rem;
  }

  input[type='text'] {
    min-height: 44px;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }
</style>
