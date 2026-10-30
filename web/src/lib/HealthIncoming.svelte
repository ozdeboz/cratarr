<script lang="ts">
  // An Incoming album waiting, Health's third level (the owner, 2026-10-24): side by side with the Music album it
  // upgrades or duplicates (the Library's list links them: `waiting`), what each is (its tracks, its format; the item's
  // line says why), and its album menu (lib/AlbumMenu), where Keep, Replace and Delete are, as everywhere; a failed conversion is tried again
  // there.
  import AlbumMenu, { offerIn } from './AlbumMenu.svelte';
  import { library, readLibrary } from './albums.svelte';
  import { albumCoverUrl, type ListedAlbum } from './api';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import { loadHealth } from './healthReport.svelte';

  import IntakeCheck from './IntakeCheck.svelte';

  let { source, album, ongone }: { source: string; album: string; ongone: () => void } = $props();

  $effect(() => {
    if (!library.view) void readLibrary(false);
  });
  const s = $derived(library.view?.sources.find((x) => x.id === source) ?? null);
  const a = $derived(s?.albums.find((x) => x.id === album) ?? null);
  /** The Music album it stands to. */
  const m = $derived.by(() => {
    if (!s) return null;
    const id = Object.entries(s.waiting).find(([, w]) => w.album === album)?.[0];
    return id ? (s.albums.find((x) => x.id === id) ?? null) : null;
  });
  const libraryName = (x: ListedAlbum) => s?.library_info.find((l) => l.id === x.library)?.name ?? '';
  const quality = (x: ListedAlbum) => [x.format, x.bitrate_kbps ? `${x.bitrate_kbps} kbps` : null].filter(Boolean).join(' · ');
  let menu = $state(false);
</script>

{#if !library.view}
  <p class="muted">…</p>
{:else if !s || !a}
  <p class="muted">No longer in the library.</p>
{:else}
  <div class="pair">
    {#each m ? [a, m] : [a] as x (x.id)}
      <div class="side">
        <span class="cover"><Cover src={albumCoverUrl(source, x.id)} /></span>
        <small class="lib">{libraryName(x)}</small>
        <b>{x.title}</b>
        {#if x.artist}<span class="muted">{x.artist}{x.year ? ` · ${x.year}` : ''}</span>{/if}
        <span>{x.tracks} {x.tracks === 1 ? 'track' : 'tracks'}{quality(x) ? ` · ${quality(x)}` : ''}</span>
      </div>
    {/each}
  </div>
  <!-- What its tags would get before it comes in (0.9.7, lib/IntakeCheck): Accept, then Keep. -->
  <IntakeCheck {source} {album} title={a.title} />
  <button class="act" onclick={() => (menu = true)}>Keep, replace or delete<Icon name="chevronRight" size={18} /></button>
  {#if menu}
    <AlbumMenu
      {source}
      album={a}
      offer={offerIn(s, a)}
      trashDays={library.view.trash_days}
      music={library.view.music ?? 'Music'}
      onclose={() => (menu = false)}
      ongone={() => {
        void readLibrary(true);
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
