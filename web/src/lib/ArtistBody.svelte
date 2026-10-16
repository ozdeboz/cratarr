<script lang="ts">
  // An artist's page under their name (decided 2026-10-16, as Symfonium's): their albums, the albums they appear on
  // (others', credited on a track), both newest first as covers, and their most played tracks (the panel's rows, `top`),
  // whose › shows all their tracks.
  import type { Snippet } from 'svelte';
  import type { ListedAlbum } from './api';
  import AlbumCard from './AlbumCard.svelte';
  import SectionHead from './SectionHead.svelte';

  let {
    source,
    name,
    albums,
    appearsOn,
    top,
    ontracks,
  }: { source: string; name: string; albums: ListedAlbum[]; appearsOn: ListedAlbum[]; top?: Snippet; ontracks: () => void } = $props();
</script>

{#snippet grid(list: ListedAlbum[])}
  <ul class="grid">
    {#each list as a (a.id)}<li><AlbumCard {source} album={a} /></li>{/each}
  </ul>
{/snippet}

{#if albums.length}
  <section aria-label="Albums">
    <SectionHead title="Albums" />
    {@render grid(albums)}
  </section>
{/if}
{#if appearsOn.length}
  <section aria-label="Appears on">
    <SectionHead title="Appears on" />
    {@render grid(appearsOn)}
  </section>
{/if}
{#if top}
  <section aria-label="Top tracks">
    <SectionHead title="Top tracks" onmore={ontracks} />
    {@render top()}
  </section>
{/if}
{#if !albums.length && !appearsOn.length && !top}
  <p class="empty">Nothing by {name} on this server.</p>
{/if}

<style>
  section {
    margin-top: 16px;
  }

  /* Three across on a phone, more as the panel widens. */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
    gap: 20px 12px;
    margin: 0;
    padding: 4px 16px 0;
    list-style: none;
  }

  li {
    min-width: 0;
  }

  .empty {
    padding: 24px 16px;
    color: var(--muted);
  }
</style>
