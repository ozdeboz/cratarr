<script lang="ts">
  // The Library's Artists (the owner, 2026-10-25: the Library chooses albums, artists or tracks beside back): the
  // libraries' albums by their artists, A to Z, as rows (how many albums and tracks, an album's cover for a face); the
  // page's search filters them by name. A tap views the artist in the panel, as an album's artist does.
  import { albumCoverUrl, type ShelfSource } from './api';
  import Cover from './Cover.svelte';
  import { plural } from './insights.svelte';
  import { libraryShown } from './sources.svelte';
  import { artistSubject, panel, viewPlaylist } from './panel.svelte';
  import { matchesSearch } from './pageView.svelte';
  import Row from './Row.svelte';

  let { sources }: { sources: ShelfSource[] } = $props();

  type Artist = { source: string; name: string; albums: number; tracks: number; cover: string | null };
  const artists = $derived.by(() => {
    const by = new Map<string, Artist>();
    for (const s of sources)
      for (const a of s.albums) {
        if (a.library && !libraryShown(s.id, a.library)) continue;
        for (const name of a.artists?.length ? a.artists : a.artist ? [a.artist] : []) {
          const key = `${s.id}\u0000${name.toLocaleLowerCase()}`;
          const x = by.get(key) ?? { source: s.id, name, albums: 0, tracks: 0, cover: albumCoverUrl(s.id, a.id) };
          x.albums++;
          x.tracks += a.tracks;
          by.set(key, x);
        }
      }
    return [...by.values()].sort((x, y) => x.name.localeCompare(y.name, undefined, { sensitivity: 'base' }));
  });
  // As every search reads (lib/search, decision 0072).
  const shown = $derived(artists.filter((a) => matchesSearch(a.name)));
</script>

<ol class="rows">
  {#each shown as a (a.source + a.name)}
    <li>
      <Row
        size="medium"
        title={a.name}
        sub={`${plural(a.albums, 'album')} · ${plural(a.tracks, 'track')}`}
        active={panel.viewed === artistSubject(a.source, a.name)}
        onclick={() => viewPlaylist(artistSubject(a.source, a.name))}
      >
        {#snippet front()}<span class="face"><Cover src={a.cover} /></span>{/snippet}
      </Row>
    </li>
  {:else}
    <li class="empty">No artists match.</li>
  {/each}
</ol>

<style>
  .rows {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .face {
    display: block;
    width: 44px;
    height: 44px;
    overflow: hidden;
    border-radius: 50%;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }
</style>
