<script lang="ts">
  // Related artists to explore, on an artist's page (0.9.2; the owner, 2026-10-25: "recommended artists, for example.
  // related artists to explore. That's the best."): those ListenBrainz finds listened to alongside, and MusicBrainz's
  // members and collaborators (the server's app/related.rs), as one row of chips, as Home's Artists to explore shows its
  // others; those that play first (the owner: "prioritize Bandcamp playable artists"). Each opens their page (W10): by
  // their id where Cratarr has them, else opened by their MusicBrainz id, else by name; never a Bandcamp album or
  // MusicBrainz's page. The server's first look runs in the background: the section's loader naming what it reads while it
  // does, asked again until it has (F8); why there are none where a source was off or didn't answer, in F1's words.
  import { artistPhotoUrl, discoverArtistPhotoUrl, getRelatedArtists, type ListRead, type RelatedArtist } from './api';
  import { openArtistPage } from './albums.svelte';
  import { artistLabel, artistOpen } from './artistIds';
  import PageLoading from './PageLoading.svelte';
  import { lookAgainAfter, sectionPlace } from './recordNotHad';
  import SectionHead from './SectionHead.svelte';
  import { toast } from './toast.svelte';

  /** The artist whose related artists these are: their id (W10), else their name. */
  let { ask }: { ask: string } = $props();

  /** The server's answer: the artists, and while it looks what it reads, or why it found none. */
  let answer = $state<({ artists: RelatedArtist[] } & ListRead) | null>(null);
  $effect(() => {
    const who = ask;
    let gone = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let n = 0;
    answer = null;
    const look = () =>
      getRelatedArtists(who).then(
        (r) => {
          if (gone) return;
          answer = r;
          if (r.looking) timer = setTimeout(look, lookAgainAfter(n++));
        },
        () => {},
      );
    look();
    return () => {
      gone = true;
      clearTimeout(timer);
    };
  });
  const artists = $derived(answer?.artists ?? []);
  /** What the section shows (F8): the artists; while read, the loader naming what it reads; once read, why there are none. */
  const place = $derived(answer && sectionPlace(answer, answer.artists.length));

  const photo = (a: RelatedArtist) => (a.id ? artistPhotoUrl(a.id) : discoverArtistPhotoUrl(a.name));
  // No picture: the chip is its name alone.
  const none = (e: Event) => ((e.currentTarget as HTMLImageElement).style.display = 'none');
  const open = (a: RelatedArtist) => openArtistPage(artistOpen(a)).catch((e) => toast.fail(e, "Their page couldn't be opened"));
</script>

{#if place}
  <section aria-label="Related artists to explore">
    <SectionHead title="Related artists to explore" />
    {#if place === 'rows'}
    <div class="chips">
      {#each artists as a (a.mbid ?? a.name)}
        <button class="chip" onclick={() => open(a)}><img src={photo(a)} alt="" onerror={none} />{artistLabel(a.name, a.number, artists.map((x) => x.name))}</button>
      {/each}
    </div>
    {:else if 'reading' in place}<PageLoading row><p class="opening">{place.reading}</p></PageLoading>{:else}<p class="empty">{place.line}</p>{/if}
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

  /* Why there are none, and what its loader waits for: as the artist page's other sections say it (lib/ArtistBody). */
  .empty {
    padding: 24px 16px;
    color: var(--muted);
  }

  .opening {
    margin: 0;
    color: var(--muted);
  }

  .chip img {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    object-fit: cover;
  }
</style>
