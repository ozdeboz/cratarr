<script lang="ts">
  // What is excluded (decision 0024, amended 2026-10-19), as a list to put back: a crate's tracks and whole albums, or
  // Cratarr's global ones (`crate` null). Each can be put back, or all at once, with Undo as excluding has. No notice
  // besides; an error in one line. In a sheet (lib/ExcludedSheet, from a crate's "N excluded") and on Settings › Taste
  // (lib/settings/TastePart), where the global ones are since 0.8.
  import { albumCoverUrl, coverUrl, crateLeftOut, excludedRows, includeTracks, putBackInCrate, type ExcludedAlbum, type Excluding, type PlaylistRow } from './api';
  import { crateName, loadCrates } from './crates.svelte';
  import { loadExcluded } from './exclusions.svelte';
  import Icon from './Icon.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import PageLoading from './PageLoading.svelte';
  import { prefs } from './prefs.svelte';
  import Row from './Row.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import { toast } from './toast.svelte';
  import TrackCover from './TrackCover.svelte';
  import { withUndo } from './undo.svelte';

  /** `crate`: whose exclusions; null: the global ones. `onchange`: something went back in (the panel reads again).
   *  `onempty`: all of it is back. */
  let { crate = null, onchange = () => {}, onempty = () => {} }: { crate?: string | null; onchange?: () => void; onempty?: () => void } = $props();

  let list = $state<{ source: string; rows: PlaylistRow[]; albums: ExcludedAlbum[] } | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  /** Put all back, asked once more (the owner, 2026-10-23). */
  let confirming = $state(false);

  async function load() {
    try {
      const l = crate ? await crateLeftOut(crate, prefs.source || undefined) : await excludedRows(prefs.source || undefined);
      list = { source: l.source, rows: l.rows, albums: 'albums' in l ? (l.albums ?? []) : [] };
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  $effect(() => {
    void crate;
    load();
  });

  const after = () => {
    loadCrates();
    if (!crate) loadExcluded();
    onchange();
  };
  async function putBack(what: Excluding) {
    const l = list;
    if (!l || !(what.ids?.length || what.albums?.length)) return;
    busy = true;
    try {
      const { undo } = crate ? await putBackInCrate(crate, l.source, what) : await includeTracks(l.source, what.ids ?? []);
      withUndo(crate ? `Put back in ${crateName(crate)}.` : 'Put back.', [undo], { after });
      after();
      await load();
      if (!list?.rows.length && !list?.albums.length) onempty();
    } catch (e) {
      toast.fail(e, "It wasn't put back");
    } finally {
      busy = false;
    }
  }
  const all = (l: NonNullable<typeof list>): Excluding => ({ ids: l.rows.flatMap((r) => (r.id ? [r.id] : [])), albums: l.albums.map((a) => a.id) });
</script>

{#if error}
  <p class="error" role="alert">{error}</p>
{:else if !list}
  <PageLoading />
{:else}
  {@const l = list}
  {#if l.rows.length + l.albums.length > 1}<MenuEntry icon="undo" label="Put all back" disabled={busy} onclick={() => (confirming = true)} />{/if}
  {#if confirming}
    <ConfirmSheet
      title="Put all back"
      text={`All ${(l.rows.length + l.albums.length).toLocaleString()} go back ${crate ? `in ${crateName(crate)}` : 'into every crate and automatic pick'}.`}
      label="Put all back"
      icon="undo"
      onconfirm={() => putBack(all(l))}
      onclose={() => (confirming = false)}
    />
  {/if}
  <ol class="rows" aria-busy={busy}>
    {#each l.albums as a (a.id)}
      <li>
        <Row size="large" title={a.title ?? 'Gone from the library'} sub={a.artist ?? ''} off={!a.title}>
          {#snippet lead()}<TrackCover src={albumCoverUrl(l.source, a.id)} />{/snippet}
          {#snippet action()}
            <button class="back" aria-label={`Put ${a.title ?? 'it'} back`} title="Put back" disabled={busy} onclick={() => putBack({ albums: [a.id] })}><Icon name="undo" size={22} /></button>
          {/snippet}
        </Row>
      </li>
    {/each}
    {#each l.rows as r, i (i)}
      <li>
        <Row size="large" title={r.name ?? r.file} sub={r.artist ?? ''} off={!r.id || r.missing}>
          {#snippet lead()}<TrackCover src={r.id && !r.missing ? coverUrl({ source: l.source, id: r.id }) : null} />{/snippet}
          {#snippet action()}
            <button class="back" aria-label={`Put ${r.name ?? r.file} back`} title="Put back" disabled={busy || !r.id} onclick={() => putBack({ ids: [r.id!] })}
              ><Icon name="undo" size={22} /></button
            >
          {/snippet}
        </Row>
      </li>
    {/each}
  </ol>
{/if}

<style>
  .rows {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    margin: 0;
    padding: 0 8px 8px;
    overflow-y: auto;
    list-style: none;
  }

  .back {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .back:hover,
  .back:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .back:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .error {
    margin: 0;
    padding: 0 24px 10px;
    color: var(--danger-ink);
    font-size: 14px;
  }
</style>
