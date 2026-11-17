<script lang="ts">
  // Related artists to explore, on an artist's page (0.9.2; the owner, 2026-10-25: "recommended artists, for example.
  // related artists to explore. That's the best."): those ListenBrainz finds listened to alongside, and MusicBrainz's
  // members and collaborators (the server's app/related.rs), as one row of chips, as Home's Artists to explore shows its
  // others; those that play first (the owner: "prioritize Bandcamp playable artists"). Each opens their page (W10): by
  // their id where Cratarr has them, else opened by their MusicBrainz id, else by name; never a Bandcamp album or
  // MusicBrainz's page. The server's first look runs in the background: the row asks again while it does.
  import { artistPhotoUrl, discoverArtistPhotoUrl, getRelatedArtists, type RelatedArtist } from './api';
  import { openArtistPage } from './albums.svelte';
  import { artistLabel, artistOpen } from './artistIds';
  import SectionHead from './SectionHead.svelte';
  import { toast } from './toast.svelte';

  /** The artist whose related artists these are: their id (W10), else their name. */
  let { ask }: { ask: string } = $props();

  /** Asked again this often while the server looks (a first look takes a minute or two), at most this many times. */
  const AGAIN_MS = 15_000;
  const TRIES = 12;

  let artists = $state<RelatedArtist[]>([]);
  $effect(() => {
    const n = ask;
    let gone = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let tries = 0;
    artists = [];
    const look = () =>
      getRelatedArtists(n).then(
        (r) => {
          if (gone) return;
          artists = r.artists;
          if (r.looking && ++tries < TRIES) timer = setTimeout(look, AGAIN_MS);
        },
        () => {},
      );
    look();
    return () => {
      gone = true;
      clearTimeout(timer);
    };
  });

  const photo = (a: RelatedArtist) => (a.id ? artistPhotoUrl(a.id) : discoverArtistPhotoUrl(a.name));
  // No picture: the chip is its name alone.
  const none = (e: Event) => ((e.currentTarget as HTMLImageElement).style.display = 'none');
  const open = (a: RelatedArtist) => openArtistPage(artistOpen(a)).catch((e) => toast.fail(e, "Their page couldn't be opened"));
</script>

{#if artists.length}
  <section aria-label="Related artists to explore">
    <SectionHead title="Related artists to explore" />
    <div class="chips">
      {#each artists as a (a.mbid ?? a.name)}
        <button class="chip" onclick={() => open(a)}><img src={photo(a)} alt="" onerror={none} />{artistLabel(a.name, a.number, artists.map((x) => x.name))}</button>
      {/each}
    </div>
  </section>
{/if}

<style>
  section {
    margin-top: 16px;
  }

  /* One row that scrolls, never wraps: Home's Artists to explore's others. */
  .chips {
    display: flex;
    gap: 8px;
    padding: 0 16px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .chip {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px 4px 4px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 14px;
    text-decoration: none;
    white-space: nowrap;
    cursor: pointer;
  }

  .chip img {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    object-fit: cover;
  }
</style>
