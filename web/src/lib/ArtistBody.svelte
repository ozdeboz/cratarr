<script lang="ts">
  // An artist's page under their name (decided 2026-10-16, as Symfonium's): About (the owner, 2026-10-23: their photo
  // and TheAudioDB's description, from the server's artist store; a tap opens all of it), their albums, the albums they
  // appear on (others', credited on a track), both newest first as covers, and their most played tracks (the panel's
  // rows, `top`), whose › shows all their tracks.
  import type { Snippet } from 'svelte';
  import { tick } from 'svelte';
  import { artistPhotoUrl, getArtistAbout, type ListedAlbum } from './api';
  import { pageAsked } from './artistAbout.svelte';
  import AlbumCard from './AlbumCard.svelte';
  import SectionHead from './SectionHead.svelte';

  let {
    source,
    name,
    albums,
    appearsOn,
    top,
    ontracks,
    profile,
  }: { source: string; name: string; albums: ListedAlbum[]; appearsOn: ListedAlbum[]; top?: Snippet; ontracks: () => void; profile?: Snippet } = $props();

  // About: Cratarr's own library only (the store knows its artists).
  let about = $state<{ id: string | null; bio: string | null; photo: boolean } | null>(null);
  let whole = $state(false);
  $effect(() => {
    const [s, n] = [source, name];
    let gone = false;
    about = null;
    whole = false;
    if (s === 'cratarr')
      getArtistAbout(n).then(
        async (a) => {
          if (gone) return;
          about = a;
          // Asked for from Home's description: unfolded, and in view.
          if (pageAsked.name === n && pageAsked.section === 'about') {
            pageAsked.name = null;
            whole = true;
            await tick();
            aboutEl?.scrollIntoView({ block: 'start', behavior: 'smooth' });
          }
        },
        () => {},
      );
    return () => (gone = true);
  });
  let aboutEl = $state<HTMLElement>();
  // Asked for from Home's fanned albums: Albums in view (once the page above it has its height).
  let albumsEl = $state<HTMLElement>();
  $effect(() => {
    if (!albumsEl || pageAsked.name !== name || pageAsked.section !== 'albums') return;
    pageAsked.name = null;
    const el = albumsEl;
    setTimeout(() => el.scrollIntoView({ block: 'start', behavior: 'smooth' }), 400);
  });
</script>

{#snippet grid(list: ListedAlbum[])}
  <ul class="grid">
    {#each list as a (a.id)}<li><AlbumCard {source} album={a} /></li>{/each}
  </ul>
{/snippet}

{#if about && (about.bio || about.photo)}
  <section aria-label="About" bind:this={aboutEl}>
    <SectionHead title="About" />
    <div class="about">
      {#if about.photo && about.id}<img class="face" src={artistPhotoUrl(about.id)} alt="" />{/if}
      {#if about.bio}
        <button class="bio" class:whole onclick={() => (whole = !whole)} aria-expanded={whole}>{about.bio}</button>
      {/if}
    </div>
    {#if about.bio}<small class="credit">TheAudioDB</small>{/if}
  </section>
{/if}
<!-- On a wide panel the albums and the artist's profile side by side, the profile all open (the owner, 2026-10-23); on
     a narrow one the profile stays in the header, to fold. -->
<div class="beside" class:with-profile={!!profile}>
  {#if albums.length}
    <section aria-label="Albums" bind:this={albumsEl}>
      <SectionHead title="Albums" />
      {@render grid(albums)}
    </section>
  {/if}
  {#if profile}<aside class="profile" aria-label="Profile">{@render profile()}</aside>{/if}
</div>
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

  .beside > .profile {
    display: none;
  }

  @container (min-width: 640px) {
    .beside.with-profile {
      display: grid;
      grid-template-columns: minmax(0, 1fr) clamp(280px, 45cqi, 480px);
      align-items: start;
      column-gap: 24px;
    }

    .beside > .profile {
      display: block;
      margin-top: 16px;
      padding-right: 16px;
    }
  }

  .about {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 4px 16px 0;
  }

  .face {
    flex: none;
    width: 72px;
    height: 72px;
    border-radius: 50%;
    object-fit: cover;
  }

  /* Four lines; a tap shows all of it (its paragraphs as written). */
  .bio {
    display: -webkit-box;
    overflow: hidden;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 0.95rem;
    line-height: 1.5;
    text-align: left;
    white-space: pre-line;
    cursor: pointer;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
  }

  .bio.whole {
    display: block;
  }

  .credit {
    display: block;
    padding: 4px 16px 0;
    color: var(--muted);
    font-size: 11px;
    text-align: right;
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
