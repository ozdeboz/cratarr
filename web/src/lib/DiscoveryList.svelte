<script lang="ts">
  // Insights › Collection › Discovery's two lists, one level deeper (the owner, 2026-10-25): the albums missing from the
  // library's artists (their discographies as MusicBrainz lists them, read so far), and what Discover leaves out (Not
  // interested, Hide, artists left out). Rows as every list's (lib/Row); the panel's search narrows them (lib/search).
  // A missing album opens from Bandcamp where it is there (looked for when opened), else its menu: Want, MusicBrainz;
  // an exclusion's menu takes it back.
  import {
    findOnBandcamp,
    getDiscoverExcluded,
    getDiscoverMissing,
    takeBackExcluded,
    wantAlbum,
    type DiscoverExcluded,
    type MissingAlbum,
  } from './api';
  import { albumIdOf, BANDCAMP } from './bandcampPreview';
  import Cover from './Cover.svelte';
  import CoverStatus from './CoverStatus.svelte';
  import CoverBusy from './CoverBusy.svelte';
  import LinkSheet from './LinkSheet.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import PageLoading from './PageLoading.svelte';
  import Row from './Row.svelte';
  import SectionHead from './SectionHead.svelte';
  import { albumSubject, viewOver } from './panel.svelte';
  import { matchesSearch } from './pageView.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  let { kind }: { kind: 'missing' | 'excluded' } = $props();

  let missing = $state<{ albums: MissingAlbum[]; artists: number } | null>(null);
  let excluded = $state<{ albums: DiscoverExcluded[]; artists: { artist: string; key: string; at: number }[] } | null>(null);
  let error = $state<string | null>(null);
  function load() {
    error = null;
    const fail = (e: unknown) => (error = e instanceof Error ? e.message : 'It could not be read');
    if (kind === 'missing') getDiscoverMissing().then((r) => (missing = r), fail);
    else getDiscoverExcluded().then((r) => (excluded = r), fail);
  }
  $effect(() => {
    void kind;
    load();
  });

  const shownMissing = $derived((missing?.albums ?? []).filter((a) => matchesSearch(a.title, a.artist)));
  const shownExcluded = $derived((excluded?.albums ?? []).filter((a) => matchesSearch(a.album, a.artist)));
  const shownArtists = $derived((excluded?.artists ?? []).filter((a) => matchesSearch(a.artist)));
  const when = (secs: number) => new Date(secs * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });

  // A missing album opened: from Bandcamp where it is there, else its menu.
  let finding = $state<string | null>(null);
  let menuFor = $state<MissingAlbum | null>(null);
  async function open(a: MissingAlbum) {
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
  async function want(a: MissingAlbum) {
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

  // An exclusion's menu: Take back (the album suggested again; an artist's every "no" gone), with Undo's word.
  let excludedMenu = $state<{ title: string; sub: string; what: { key: string } | { artist: string } } | null>(null);
  async function takeBack(what: { key: string } | { artist: string }) {
    excludedMenu = null;
    try {
      await takeBackExcluded(what);
      withUndo('Taken back.', []);
      load();
    } catch (e) {
      toast.fail(e, 'Could not take it back');
    }
  }
</script>

{#if error}
  <p class="note" role="alert">{error}</p>
{:else if kind === 'missing'}
  {#if !missing}
    <PageLoading />
  {:else if !missing.albums.length}
    <p class="note">Nothing missing from the {missing.artists} artists read so far.</p>
  {:else}
    <p class="note">From the discographies of {missing.artists} of your artists, as MusicBrainz lists them.</p>
    <ul class="rows">
      {#each shownMissing as a (a.release_group)}
        <li>
          <Row size="large" title={a.title} label={`Open ${a.title}`} onclick={() => open(a)}>
            {#snippet front()}<span class="art"><Cover src={a.cover} /><CoverStatus mark={a.request ?? (asked.includes(a.release_group) ? { state: 'wanted' } : null)} small />{#if finding === a.release_group}<CoverBusy small />{/if}</span>{/snippet}
            {#snippet sub()}{a.artist}{a.year ? ` · ${a.year}` : ''}{a.kind && a.kind !== 'Album' ? ` · ${a.kind}` : ''}{#if asked.includes(a.release_group)}<span class="asked"> · asked for</span>{/if}{/snippet}
          </Row>
        </li>
      {/each}
    </ul>
  {/if}
{:else if !excluded}
  <PageLoading />
{:else if !excluded.albums.length && !excluded.artists.length}
  <p class="note">Discover leaves nothing out.</p>
{:else}
  {#if shownArtists.length}
    <SectionHead title="Artists left out" />
    <ul class="rows">
      {#each shownArtists as x (x.key)}
        <li>
          <Row size="large" title={x.artist} label={`${x.artist}: its menu`} onclick={() => (excludedMenu = { title: x.artist, sub: 'Left out', what: { artist: x.key } })}>
            {#snippet sub()}Not interested twice · {when(x.at)}{/snippet}
          </Row>
        </li>
      {/each}
    </ul>
  {/if}
  {#if shownExcluded.length}
    <SectionHead title="Albums" />
    <ul class="rows">
      {#each shownExcluded as a (a.key)}
        <li>
          <Row size="large" title={a.album} label={`${a.album}: its menu`} onclick={() => (excludedMenu = { title: a.album, sub: a.artist, what: { key: a.key } })}>
            {#snippet front()}<span class="art"><Cover src={a.cover ?? undefined} /></span>{/snippet}
            {#snippet sub()}{a.artist} · {a.how === 'no' ? 'Not interested' : 'Hidden'} · {when(a.at)}{/snippet}
          </Row>
        </li>
      {/each}
    </ul>
  {/if}
{/if}

{#if menuFor}
  {@const a = menuFor}
  <MenuSheet label={a.title} title={a.title} images={a.cover ? [a.cover] : []} chips={[a.artist, ...(a.year ? [String(a.year)] : [])]} copy={`${a.artist} – ${a.title}`} onclose={() => (menuFor = null)}>
    <MenuCards>
      <MenuGroup label="Discover">
        {#if asked.includes(a.release_group)}
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
{#if excludedMenu}
  {@const x = excludedMenu}
  <MenuSheet label={x.title} title={x.title} chips={[x.sub]} onclose={() => (excludedMenu = null)}>
    <MenuCards>
      <MenuGroup label="Discover">
        <MenuEntry icon="undo" label="Take back" onclick={() => takeBack(x.what)} />
      </MenuGroup>
    </MenuCards>
  </MenuSheet>
{/if}
{#if link}{@const l = link}<LinkSheet url={l.url} name={l.name} onclose={() => (link = null)} />{/if}

<style>
  .note {
    margin: 0 16px 12px;
    color: var(--muted);
    font-size: 14px;
  }

  .rows {
    margin: 0;
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

  .asked {
    color: var(--accent-ink);
  }
</style>
