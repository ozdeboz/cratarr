<script lang="ts">
  // Download… as files (decision 0058, amended 0.10; lib/DownloadStep): one step for every object and selection,
  // inside its menu sheet (lib/MenuSheet), and the panel's Export tab under its playlists. What it will hold (how many
  // tracks, about how large, how many missing are left out); for playlists with their tracks or only the playlist
  // files, several as they are or combined into one; for a list of tracks with a playlist file or not; the format and
  // the layout; in the app, one zip or the files one by one; then Download, which readies it on the server and starts
  // the download (a link: the browser shows its progress, and closing it cancels; the files one by one, the app writes
  // into a folder picked there). The choices are kept in this browser (lib/export.svelte).
  import { exportDownloadUrl, exportPreview, prepareExport, type ExportPreview, type ExportWhat } from './api';
  import { inApp, saveExport } from './app.svelte';
  import Card from './Card.svelte';
  import ChoiceList from './ChoiceList.svelte';
  import { exporting } from './export.svelte';
  import { BITRATES, exportLine, exportReady, exportRequest, FORMATS, several, suggestedName, withTracks } from './exportChoices';
  import Icon from './Icon.svelte';
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

<div class="parts">
  <p class="sum" aria-live="polite">
    {#if failed}<span class="error">{failed}</span>{:else if preview}{exportLine(preview)}{/if}
  </p>

  <!-- The format first (the owner: the first and most important), then the switches side by side. -->
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
    <Switch card label="Include tracks" checked={exporting.tracks} onchange={(on) => (exporting.tracks = on)} />
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
  {:else}
    <Switch card label="Include a playlist file" checked={exporting.m3u} onchange={(on) => (exporting.m3u = on)} />
  {/if}
  {#if tracks}
    <!-- The layout as a switch (the owner): on, Artist and album folders; off, one folder. -->
    <Switch card label="Preserve directory layout" checked={exporting.layout === 'folders'} onchange={(on) => (exporting.layout = on ? 'folders' : 'flat')} />
  {/if}

  <button class="go" disabled={!ready || starting} aria-busy={starting} onclick={start}><Icon name="arrowDownToLine" size={20} /><span>Download</span></button>
</div>

<style>
  /* Each choice a row on a card of its own (lib/SettingRow, lib/Card, lib/Switch), as View's. */
  .parts {
    display: grid;
    gap: 4px;
    padding: 4px 16px 16px;
  }

  .sum {
    min-height: 1.4em;
    margin: 0 0 8px;
    font-size: 0.9rem;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
    text-align: center;
  }

  .error {
    color: var(--danger-ink);
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

  .go {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-height: 44px;
    margin-top: 12px;
    border: 0;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .go:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
