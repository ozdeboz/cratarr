<script lang="ts">
  // Find more music (the owner, 2026-10-25: where a search finds nothing anywhere in Cratarr, "Find More Music" closes
  // the search and loads a MusicBrainz query or fetch page): the words looked up beyond Cratarr (decision 0076: Beyond's
  // Show all; MusicBrainz and Discogs together, merged, each album's sources marked, Discogs' with its attribution; its
  // albums and artists, the surest first, what the library has marked), more as it scrolls; each album opens its record's
  // page (W9: a record not had, made one at open where none holds it yet; Want in its header). The words ride in the address
  // (`#/find?q=`); the pill's search asks again.
  import { searchBeyond, type BeyondFound } from './lib/api';
  import { keepWhile } from './lib/keepWhile.svelte';
  import Notice from './lib/Notice.svelte';
  import { recordsTitle } from './lib/recordTypeRules';
  import { wordHere } from './lib/addresses';
  import Cover from './lib/Cover.svelte';
  import CoverStatus from './lib/CoverStatus.svelte';
  import CoverBusy from './lib/CoverBusy.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import Row from './lib/Row.svelte';
  import SectionHead from './lib/SectionHead.svelte';
  import { pageView } from './lib/pageView.svelte';
  import { openArtistPage, openNotHad } from './lib/albums.svelte';
  import { artistLabel, artistOpen } from './lib/artistIds';
  import { beyondKey } from './lib/beyondRows';
  import { toast } from './lib/toast.svelte';

  /** The words: the pill's while it searches, else the address's. */
  const fromAddress = () => wordHere('q') ?? '';
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
  /** A next page that failed: said with Try again, the list not ended there (SB29: errors are never shown as nothing). */
  let pageFailed = $state<string | null>(null);
  $effect(() => {
    const now = q;
    error = null;
    pageFailed = null;
    if (!now) return;
    let gone = false;
    const t = setTimeout(() => {
      searchBeyond(now, PAGE, 0, (r) => !gone && (found = { q: now, r, done: false })).then(
        (r) => !gone && (found = { q: now, r, done: r.albums.length < PAGE }),
        (e) => !gone && (error = e instanceof Error ? e.message : 'The search could not be asked'),
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  // New words keep what was shown while they are asked, the loader in its place once due (the one loading rule, SB29: the
  // results were swapped for the loader at every key, though the ask waits 300 ms), as Everywhere's results keep theirs.
  const kept = keepWhile(
    () => found,
    () => !!q && !error && found?.q !== q,
    () => q,
  );
  const shown = $derived(kept.loader ? null : kept.value);
  type Album = BeyondFound['albums'][number];
  /** A row's key: its record, else what its source knows it by (two of one name are two rows, C57; lib/beyondRows). */
  const keyOf = (a: Album) => beyondKey(a);
  const SOURCE = { musicbrainz: 'MusicBrainz', discogs: 'Discogs' } as const;
  /** The next page, as the end of the list comes into sight (a server before 0.9.13 has one). */
  let more = false;
  async function nextPage() {
    const f = found;
    if (!f || f.done || more || f.q !== q) return;
    more = true;
    try {
      const r = await searchBeyond(f.q, PAGE, f.r.albums.length);
      if (found !== f) return;
      const seen = new Set(f.r.albums.map(keyOf));
      const fresh = r.albums.filter((a) => !seen.has(keyOf(a)));
      found = { q: f.q, r: { ...f.r, albums: [...f.r.albums, ...fresh] }, done: !fresh.length || r.albums.length < PAGE };
    } catch (e) {
      if (found === f) pageFailed = e instanceof Error ? e.message : 'The search could not be asked';
    } finally {
      more = false;
    }
  }
  const moreAgain = () => {
    pageFailed = null;
    void nextPage();
  };
  function nearEnd(el: Element) {
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && void nextPage(), { rootMargin: '400px' });
    io.observe(el);
    return () => io.disconnect();
  }

  // Opened: its record's page (W9, lib/openRecord), made one first where no record holds it yet.
  let finding = $state<string | null>(null);
  async function open(a: Album) {
    if (finding) return;
    finding = keyOf(a);
    try {
      await openNotHad({ record: a.record, release_group: a.release_group, discogs_master: a.discogs, title: a.title, artist: a.artist, year: a.year, bandcamp: a.bandcamp });
    } catch (e) {
      toast.fail(e, "It couldn't be opened");
    } finally {
      finding = null;
    }
  }
  /** An artist opened (W10): their page, by their id where Cratarr has them, else opened by their MusicBrainz id. */
  const openArtist = (x: BeyondFound['artists'][number]) =>
    openArtistPage(artistOpen({ id: x.artist, mbid: x.mbid, name: x.name })).catch((e) => toast.fail(e, "Their page couldn't be opened"));
</script>

<main>
  <!-- SB12: what it asks, in its words; the record word rule's heading; no line saying what the page is. -->
  {#if !q}
    <p class="note">Search MusicBrainz and Discogs</p>
  {:else if error}
    <p class="note" role="alert">{error}</p>
  {:else if !shown}
    <PageLoading />
  {:else if !shown.r.albums.length && !shown.r.artists.length}
    <p class="note">Nothing found for “{shown.q}”.</p>
  {:else}
    {@const artists = shown.r.artists}
    {#if artists.length}
      <SectionHead title="Artists" />
      <ul class="rows">
        {#each artists as x (x.mbid)}
          <li>
            <Row
              size="large"
              title={artistLabel(x.name, x.number, artists.map((a) => a.name))}
              label={`Open ${x.name}`}
              onclick={() => openArtist(x)}
            >
              {#snippet sub()}{x.disambiguation ?? ''}{#if x.have}<span class="have">{x.disambiguation ? ' · ' : ''}in your library</span>{/if}{/snippet}
            </Row>
          </li>
        {/each}
      </ul>
    {/if}
    {#if shown.r.albums.length}
      <SectionHead title={recordsTitle(shown.r.albums.map((a) => a.type))} />
      <ul class="rows">
        {#each shown.r.albums as a (keyOf(a))}
          <li>
            <Row size="large" title={a.title} label={`Open ${a.title}`} onclick={() => open(a)}>
              {#snippet front()}<span class="art"><Cover src={a.cover} /><CoverStatus mark={a.request ?? null} small />{#if finding === keyOf(a)}<CoverBusy small />{/if}</span>{/snippet}
              {#snippet sub()}{a.artist}{a.year ? ` · ${a.year}` : ''} · {a.sources.map((x) => SOURCE[x]).join(' · ')}{#if a.have}<span class="have"> · in your library</span>{/if}{/snippet}
            </Row>
          </li>
        {/each}
      {#if !shown.done && !pageFailed}<li class="more" {@attach nearEnd}></li>{/if}
      </ul>
      {#if pageFailed}<Notice text={pageFailed} onretry={moreAgain} away="Find more music needs Cratarr." />{/if}
      <!-- Discogs' data shown with its attribution (its terms; 0076). -->
      {#if shown.r.albums.some((a) => a.sources.includes('discogs'))}<p class="note credit">Data from <a href="https://www.discogs.com" target="_blank" rel="noopener noreferrer">Discogs</a></p>{/if}
    {/if}
  {/if}
</main>


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
