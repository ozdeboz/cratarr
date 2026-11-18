<script lang="ts">
  // An Incoming album waiting, Health's third level (the owner, 2026-10-24): side by side with the copy it stands against,
  // both copies of one record (C37a: the Incoming copy's `against` names that copy's library), what each is (its tracks, its
  // format; the item's line says why), and its album menu (lib/AlbumMenu), where Keep, Replace and Delete are, as everywhere; a failed conversion is tried again
  // there.
  import { offers } from './capabilities.svelte';
  import AlbumMenu from './AlbumMenu.svelte';
  import { library, readLibrary } from './albums.svelte';
  import { albumCoverUrl, type AlbumCopy } from './api';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import { loadHealth } from './healthReport.svelte';

  import { openRecordPlan } from './fileRuns.svelte';
  import IntakeCard from './IntakeCard.svelte';

  let { source, album, ongone }: { source: string; album: string; ongone: () => void } = $props();

  $effect(() => {
    if (!library.view) readLibrary(false).catch(() => {});
  });
  const s = $derived(library.view?.sources.find((x) => x.id === source) ?? null);
  const a = $derived(s?.albums.find((x) => x.id === album) ?? null);
  type Side = Pick<AlbumCopy, 'library' | 'tracks' | 'format' | 'kbps'>;
  /** The copy waiting and the copy it stands against, each one side; else the record as it plays (another server's: as listed). */
  const sides = $derived.by((): Side[] => {
    if (!a) return [];
    const waiting = a.copies?.find((c) => c.against);
    const kept = waiting && a.copies?.find((c) => c.library === waiting.against!.library);
    if (waiting && kept) return [waiting, kept];
    return [a.copies?.find((c) => c.plays) ?? { library: a.library ?? '', tracks: a.tracks, format: a.format, kbps: a.bitrate_kbps }];
  });
  const libraryName = (x: Side) => s?.library_info.find((l) => l.id === x.library)?.name ?? '';
  const quality = (x: Side) => [x.format, x.kbps ? `${x.kbps} kbps` : null].filter(Boolean).join(' · ');
  let menu = $state(false);
</script>

{#if !library.view}
  <p class="muted">…</p>
{:else if !s || !a}
  <p class="muted">No longer in the library.</p>
{:else}
  <div class="pair">
    {#each sides as x (x.library)}
      <div class="side">
        <span class="cover"><Cover src={albumCoverUrl(source, a.id)} /></span>
        <small class="lib">{libraryName(x)}</small>
        <b>{a.title}</b>
        {#if a.artist}<span class="muted">{a.artist}{a.year ? ` · ${a.year}` : ''}</span>{/if}
        <span>{x.tracks} {x.tracks === 1 ? 'track' : 'tracks'}{quality(x) ? ` · ${quality(x)}` : ''}</span>
      </div>
    {/each}
  </div>
  <!-- Cratarr's own: its intake card, checks on top, tracks under (lib/IntakeCard). -->
  {#if source === 'cratarr'}<IntakeCard {source} {album} />{/if}
  <!-- What its tags would get before it comes in: reviewed on the sheet, written as a run (decision 0092, 0.14). -->
  {#if offers('intake_check') && source === 'cratarr'}<button class="act" onclick={() => openRecordPlan('intake', album)}>Before adding<Icon name="chevronRight" size={18} /></button>{/if}
  <button class="act" onclick={() => (menu = true)}>Add, replace or delete<Icon name="chevronRight" size={18} /></button>
  {#if menu}
    <AlbumMenu
      {source}
      album={a}
      onclose={() => (menu = false)}
      ongone={() => {
        readLibrary(true).catch(() => {});
        loadHealth();
        ongone();
      }}
    />
  {/if}
{/if}

<style>
  p {
    margin: 0;
  }

  .muted {
    color: var(--muted);
  }

  /* Incoming beside Music, each its cover over what it is. */
  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
    max-width: 560px;
  }

  .side {
    display: grid;
    align-content: start;
    gap: 2px;
    min-width: 0;
  }

  .side b,
  .side span {
    overflow-wrap: anywhere;
  }

  .side b {
    font-weight: 500;
  }

  .cover {
    display: block;
    aspect-ratio: 1;
    margin-bottom: 6px;
    overflow: hidden;
    border-radius: 10px;
  }

  .lib {
    color: var(--muted);
    font-size: 0.8rem;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .act {
    justify-self: start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px 10px 18px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
</style>
