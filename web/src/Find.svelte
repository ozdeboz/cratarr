<script lang="ts">
  // Find more music (the owner, 2026-10-25: where a search finds nothing anywhere in Cratarr, "Find More Music" closes
  // the search and loads a MusicBrainz query or fetch page): the words looked up beyond Cratarr (decision 0076: Beyond's
  // Show all; MusicBrainz and Discogs together, merged, each album's sources marked, Discogs' with its attribution; its
  // albums and artists, the surest first, what the library has marked), more as it scrolls; each album to hear from
  // Bandcamp where it is there (looked for when opened), else its menu to Want it. The words ride in the address
  // (`#/find?q=`); the pill's search asks again.
  import { searchBeyond, type BeyondFound } from './lib/api';
  import Cover from './lib/Cover.svelte';
  import CoverStatus from './lib/CoverStatus.svelte';
  import CoverBusy from './lib/CoverBusy.svelte';
  import ElsewhereAlbum, { elsewhereKey } from './lib/ElsewhereAlbum.svelte';
  import LinkSheet from './lib/LinkSheet.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import Row from './lib/Row.svelte';
  import SectionHead from './lib/SectionHead.svelte';
  import { pageView } from './lib/pageView.svelte';
  import { goToArtist } from './lib/albums.svelte';

  /** The words: the pill's while it searches, else the address's. */
  const fromAddress = () => new URLSearchParams(location.hash.split('?')[1] ?? '').get('q') ?? '';
  let addressed = $state(fromAddress());
  $effect(() => {
    const on = () => (addressed = fromAddress());
    window.addEventListener('hashchange', on);
    return () => window.removeEventListener('hashchange', on);
  });
  const q = $derived(pageView.query.trim() || addressed);

  let found = $state<{ q: string; r: BeyondFound; done: boolean } | null>(null);
  /** How many albums a page of Beyond brings. */
  const PAGE = 40;
  let error = $state<string | null>(null);
  $effect(() => {
    const now = q;
    error = null;
    if (!now) return;
    let gone = false;
    const t = setTimeout(() => {
      searchBeyond(now, PAGE).then(
        (r) => !gone && (found = { q: now, r, done: r.albums.length < PAGE }),
        (e) => !gone && (error = e instanceof Error ? e.message : 'The search could not be asked'),
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  type Album = BeyondFound['albums'][number];
  // As lib/ElsewhereAlbum knows one while it is looked for and once asked for.
  const keyOf = (a: Album) => elsewhereKey({ release_group: a.release_group ?? null, artist: a.artist, title: a.title });
  const SOURCE = { musicbrainz: 'MusicBrainz', discogs: 'Discogs' } as const;
  /** The next page, as the end of the list comes into sight (a server before 0.9.13 has one). */
  let more = false;
  async function nextPage() {
    const f = found;
    if (!f || f.done || more) return;
    more = true;
    try {
      const r = await searchBeyond(f.q, PAGE, f.r.albums.length);
      if (found !== f) return;
      const seen = new Set(f.r.albums.map(keyOf));
      const fresh = r.albums.filter((a) => !seen.has(keyOf(a)));
      found = { q: f.q, r: { ...f.r, albums: [...f.r.albums, ...fresh] }, done: !fresh.length || r.albums.length < PAGE };
    } catch {
      if (found === f) found = { ...f, done: true };
    } finally {
      more = false;
    }
  }
  function nearEnd(el: Element) {
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && void nextPage(), { rootMargin: '400px' });
    io.observe(el);
    return () => io.disconnect();
  }

  // Opened: on Bandcamp in the panel, else its menu (lib/ElsewhereAlbum, as the global search's missing albums).
  let elsewhere = $state<ElsewhereAlbum>();
  let finding = $state<string | null>(null);
  let asked = $state<string[]>([]);
  const open = (a: Album) =>
    elsewhere?.open({ release_group: a.release_group ?? null, title: a.title, artist: a.artist, year: a.year, cover: a.cover, have: a.have, bandcamp: a.bandcamp ?? null, discogs_url: a.discogs_url ?? null });
  let link = $state<{ url: string; name: string } | null>(null);
</script>

<main>
  {#if !q}
    <p class="note">Search for an artist or an album to find it on MusicBrainz.</p>
  {:else if error}
    <p class="note" role="alert">{error}</p>
  {:else if !found || found.q !== q}
    <PageLoading />
  {:else if !found.r.albums.length && !found.r.artists.length}
    <p class="note">Nothing found for “{q}”.</p>
  {:else}
    <p class="note">Beyond Cratarr for “{q}”.</p>
    {#if found.r.artists.length}
      <SectionHead title="Artists" />
      <ul class="rows">
        {#each found.r.artists as x (x.mbid)}
          <li>
            <Row
              size="large"
              title={x.name}
              label={`Open ${x.name}`}
              onclick={() => (x.have ? goToArtist('cratarr', x.name) : (link = { url: `https://musicbrainz.org/artist/${x.mbid}`, name: 'MusicBrainz' }))}
            >
              {#snippet sub()}{x.disambiguation ?? ''}{#if x.have}<span class="have">{x.disambiguation ? ' · ' : ''}in your library</span>{/if}{/snippet}
            </Row>
          </li>
        {/each}
      </ul>
    {/if}
    {#if found.r.albums.length}
      <SectionHead title="Albums" />
      <ul class="rows">
        {#each found.r.albums as a (keyOf(a))}
          <li>
            <Row size="large" title={a.title} label={`Open ${a.title}`} onclick={() => open(a)}>
              {#snippet front()}<span class="art"><Cover src={a.cover} /><CoverStatus mark={a.request ?? (asked.includes(keyOf(a)) ? { state: 'wanted' } : null)} small />{#if finding === keyOf(a)}<CoverBusy small />{/if}</span>{/snippet}
              {#snippet sub()}{a.artist}{a.year ? ` · ${a.year}` : ''} · {a.sources.map((x) => SOURCE[x]).join(' · ')}{#if a.have}<span class="have"> · in your library</span>{:else if asked.includes(keyOf(a))}<span class="have"> · asked for</span>{/if}{/snippet}
            </Row>
          </li>
        {/each}
      {#if !found.done}<li class="more" {@attach nearEnd}></li>{/if}
      </ul>
      <!-- Discogs' data shown with its attribution (its terms; 0076). -->
      {#if found.r.albums.some((a) => a.sources.includes('discogs'))}<p class="note credit">Data from <a href="https://www.discogs.com" target="_blank" rel="noopener noreferrer">Discogs</a></p>{/if}
    {/if}
  {/if}
</main>

<ElsewhereAlbum bind:this={elsewhere} bind:finding bind:asked />
{#if link}{@const l = link}<LinkSheet url={l.url} name={l.name} onclose={() => (link = null)} />{/if}

<style>
  main {
    padding: 14px clamp(12px, 2vw, 32px) 48px;
  }

  .note {
    margin: 0 0 12px;
    color: var(--muted);
    font-size: 14px;
  }

  .rows {
    margin: 0 -12px 12px;
    padding: 0;
    list-style: none;
  }

  .art {
    position: relative;
    display: block;
    width: 48px;
    height: 48px;
    overflow: hidden;
    border-radius: 6px;
  }

  .have {
    color: var(--accent-ink);
  }

  .credit {
    font-size: 11px;
  }

  .credit a {
    color: inherit;
  }

</style>
