<script lang="ts">
  // The panel's Export tab (decisions 0007, 0025): the selected playlists (each can be left out; more are ticked with the
  // Playlists page's Select: beside the page, on its chips; full-screen, Add playlists goes there), the three options,
  // what it will hold, and the download (a link: the browser shows its progress, and closing it cancels).
  import { exportPreview, exportUrl, type ExportChoice, type ExportPreview } from './api';
  import { exporting } from './export.svelte';
  import Icon from './Icon.svelte';
  import { inSidebar } from './live.svelte';
  import { shownPlaylists } from './output.svelte';
  import { addMore, leaveOut } from './select.svelte';

  let { ids }: { ids: string[] } = $props();

  const all = $derived(shownPlaylists());
  const picked = $derived(ids.map((id) => all.find((p) => p.id === id)).filter((p) => !!p));

  // The combined playlist's name follows the selection until it is typed in.
  const suggested = $derived.by(() => {
    const names = picked.map((p) => p.name);
    return names.length <= 3 ? names.join(' + ') : `${names.slice(0, 2).join(' + ')} and ${names.length - 2} more`;
  });
  $effect(() => {
    const s = suggested;
    if (!exporting.named) exporting.name = s;
  });

  const choice = $derived<ExportChoice>({
    playlists: picked.map((p) => p.id),
    tracks: exporting.tracks,
    flat: exporting.flat,
    combine: exporting.combine ? exporting.name.trim() : null,
  });
  const ready = $derived(picked.length > 0 && (!exporting.combine || exporting.name.trim() !== ''));

  // What it will hold, asked for again 300 ms after the last change.
  let preview = $state<ExportPreview | null>(null);
  let failed = $state<string | null>(null);
  $effect(() => {
    const c = choice;
    if (!c.playlists.length) {
      preview = null;
      return;
    }
    let gone = false;
    const t = setTimeout(() => {
      exportPreview(c).then(
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

  const size = (b: number) =>
    b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : b >= 1e6 ? `${Math.round(b / 1e6)} MB` : `${Math.max(1, Math.round(b / 1e3))} kB`;
  const label = $derived(exporting.combine || picked.length === 1 ? 'Export playlist' : `Export ${picked.length} playlists`);
</script>

<div class="tab">
  <ul class="lists" aria-label="Playlists to export">
    {#each picked as p (p.id)}
      <li>
        <span class="name">{p.name}</span><span class="n">{p.count}</span>
        <button class="out" aria-label={`Leave out ${p.name}`} onclick={() => leaveOut(p.id)}><Icon name="close" size={18} /></button>
      </li>
    {/each}
  </ul>
  {#if !inSidebar()}
    <button class="more" onclick={addMore}><Icon name="plus" size={18} /><span>Add playlists</span></button>
  {/if}

  <div class="label">Options</div>
  <div class="options">
    <label><input type="checkbox" bind:checked={exporting.tracks} />Include tracks</label>
    <label class="sub" class:off={!exporting.tracks}
      ><input type="checkbox" bind:checked={exporting.flat} disabled={!exporting.tracks} />All tracks in one folder</label
    >
    <label><input type="checkbox" bind:checked={exporting.combine} />Combine into one playlist</label>
    {#if exporting.combine}
      <input
        type="text"
        class="sub"
        aria-label="Name of the combined playlist"
        maxlength="100"
        bind:value={exporting.name}
        oninput={() => (exporting.named = true)}
      />
    {/if}
  </div>

  <p class="sum" aria-live="polite">
    {#if failed}
      <span class="error">{failed}</span>
    {:else if preview}
      {preview.tracks.toLocaleString()}
      {preview.tracks === 1 ? 'track' : 'tracks'}{#if preview.bytes !== null}&nbsp;· {size(preview.bytes)}{/if}{#if preview.missing > 0}<br /><span
          class="missing">{preview.missing.toLocaleString()} missing {preview.missing === 1 ? 'track' : 'tracks'} left out</span
        >{/if}
    {/if}
  </p>

  {#if ready}
    <a class="go" href={exportUrl(choice)} download><Icon name="download" size={20} /><span>{label}</span></a>
  {:else}
    <span class="go off" aria-disabled="true"><Icon name="download" size={20} /><span>{label}</span></span>
  {/if}
</div>

<style>
  .tab {
    padding: 4px 16px 0;
  }

  .label {
    margin: 18px 0 6px;
    font-size: 13px;
    color: var(--muted);
  }

  .lists {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .lists li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding-left: 12px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }

  .lists li + li {
    margin-top: 6px;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .n {
    flex: none;
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    opacity: 0.65;
  }

  .out {
    flex: none;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border: 0;
    background: none;
    color: inherit;
    opacity: 0.7;
    cursor: pointer;
  }

  .more {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 40px;
    margin-top: 8px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .options {
    display: grid;
    gap: 2px;
  }

  .options label {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    cursor: pointer;
  }

  .options .sub {
    margin-left: 28px;
  }

  .options label.off {
    opacity: 0.45;
    cursor: default;
  }

  input[type='checkbox'] {
    width: 18px;
    height: 18px;
    margin: 0;
    accent-color: var(--accent);
  }

  input[type='text'] {
    min-height: 40px;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .sum {
    min-height: 2.6em;
    margin: 18px 0 10px;
    font-size: 0.9rem;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }

  .missing {
    color: var(--warn);
  }

  .error {
    color: var(--danger, #c33);
  }

  .go {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-height: 44px;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
    text-decoration: none;
  }

  .go.off {
    opacity: 0.5;
  }
</style>
