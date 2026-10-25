<script lang="ts">
  // Find more music (the owner, 2026-10-25: where a search finds nothing anywhere in Cratarr, "Find More Music" closes
  // the search and loads a MusicBrainz query or fetch page): the words looked up on MusicBrainz (its albums and artists,
  // the surest first, what the library has marked), each album to hear from Bandcamp where it is there (looked for when
  // opened), else its menu to Want it. The words ride in the address (`#/find?q=`); the pill's search asks again.
  import { findOnBandcamp, musicBrainzSearch, wantAlbum, type MusicBrainzFound } from './lib/api';
  import { albumIdOf, BANDCAMP } from './lib/bandcampPreview';
  import Cover from './lib/Cover.svelte';
  import LinkSheet from './lib/LinkSheet.svelte';
  import MenuCards from './lib/MenuCards.svelte';
  import MenuEntry from './lib/MenuEntry.svelte';
  import MenuGroup from './lib/MenuGroup.svelte';
  import MenuSheet from './lib/MenuSheet.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import Row from './lib/Row.svelte';
  import SectionHead from './lib/SectionHead.svelte';
  import { albumSubject, viewOver } from './lib/panel.svelte';
  import { pageView } from './lib/pageView.svelte';
  import { goToArtist } from './lib/albums.svelte';
  import { toast } from './lib/toast.svelte';

  /** The words: the pill's while it searches, else the address's. */
  const fromAddress = () => new URLSearchParams(location.hash.split('?')[1] ?? '').get('q') ?? '';
  let addressed = $state(fromAddress());
  $effect(() => {
    const on = () => (addressed = fromAddress());
    window.addEventListener('hashchange', on);
    return () => window.removeEventListener('hashchange', on);
  });
  const q = $derived(pageView.query.trim() || addressed);

  let found = $state<{ q: string; r: MusicBrainzFound } | null>(null);
  let error = $state<string | null>(null);
  $effect(() => {
    const now = q;
    error = null;
    if (!now) return;
    let gone = false;
    const t = setTimeout(() => {
      musicBrainzSearch(now).then(
        (r) => !gone && (found = { q: now, r }),
        (e) => !gone && (error = e instanceof Error ? e.message : 'MusicBrainz could not be asked'),
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  type Album = MusicBrainzFound['albums'][number];

  let finding = $state<string | null>(null);
  let menuFor = $state<Album | null>(null);
  async function open(a: Album) {
    finding = a.release_group;
    try {
      const { page } = await findOnBandcamp(a.artist, a.title, a.release_group);
      if (page) viewOver(albumSubject(BANDCAMP, albumIdOf(page)));
      else menuFor = a;
    } catch {
      menuFor = a;
    } finally {
      finding = null;
    }
  }
  let asked = $state<string[]>([]);
  async function want(a: Album) {
    menuFor = null;
    try {
      const { request } = await wantAlbum({ release_group: a.release_group, artist: a.artist, album: a.title, year: a.year, cover: a.cover });
      asked = [...asked, a.release_group];
      if (request.fetched?.startsWith('Not handed over')) toast.show({ text: request.fetched, tone: 'error' });
    } catch (e) {
      toast.fail(e, 'Could not ask for it');
    }
  }
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
    <p class="note">MusicBrainz knows nothing for “{q}”.</p>
  {:else}
    <p class="note">On MusicBrainz for “{q}”.</p>
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
        {#each found.r.albums as a (a.release_group)}
          <li>
            <Row size="large" title={a.title} label={`Open ${a.title}`} onclick={() => open(a)}>
              {#snippet front()}<span class="art"><Cover src={a.cover} /></span>{/snippet}
              {#snippet sub()}{a.artist}{a.year ? ` · ${a.year}` : ''}{a.kind && a.kind !== 'Album' ? ` · ${a.kind}` : ''}{#if a.have}<span class="have"> · in your library</span>{:else if asked.includes(a.release_group)}<span class="have"> · asked for</span>{/if}{/snippet}
              {#snippet action()}{#if finding === a.release_group}<span class="have" aria-label="Looking on Bandcamp">…</span>{/if}{/snippet}
            </Row>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</main>

{#if menuFor}
  {@const a = menuFor}
  <MenuSheet label={a.title} title={a.title} images={a.cover ? [a.cover] : []} chips={[a.artist, ...(a.year ? [String(a.year)] : [])]} copy={`${a.artist} – ${a.title}`} onclose={() => (menuFor = null)}>
    <MenuCards>
      <MenuGroup label="Discover">
        {#if a.have}
          <MenuEntry icon="disc" label="In your library" disabled onclick={() => {}} />
        {:else if asked.includes(a.release_group)}
          <MenuEntry icon="inbox" label="Requested" onclick={() => ((menuFor = null), (location.hash = '#/requests'))} />
        {:else}
          <MenuEntry icon="circlePlus" label="Want" onclick={() => want(a)} />
        {/if}
      </MenuGroup>
      <MenuGroup>
        <MenuEntry icon="externalLink" label="MusicBrainz" onclick={() => ((link = { url: `https://musicbrainz.org/release-group/${a.release_group}`, name: 'MusicBrainz' }), (menuFor = null))} />
        <MenuEntry icon="search" label="Search YouTube" onclick={() => ((link = { url: `https://www.youtube.com/results?search_query=${encodeURIComponent(`${a.artist} ${a.title}`)}`, name: 'YouTube' }), (menuFor = null))} />
      </MenuGroup>
    </MenuCards>
  </MenuSheet>
{/if}
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
    display: block;
    width: 48px;
    height: 48px;
    overflow: hidden;
    border-radius: 6px;
  }

  .have {
    color: var(--accent-ink);
  }
</style>
