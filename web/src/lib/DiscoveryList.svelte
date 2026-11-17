<script lang="ts">
  // Insights › Collection › Discovery's two lists, one level deeper (the owner, 2026-10-25): the albums missing from the
  // library's artists (their discographies as MusicBrainz lists them, read so far), and what Discover leaves out (Not
  // interested, Hide, artists left out). Rows as every list's (lib/Row); the panel's search narrows them (lib/search).
  // A missing album opens its record's page (W9: a record not had, Want in its header); an exclusion's menu takes it back.
  import { getDiscoverExcluded, getDiscoverMissing, takeBackExcluded, type DiscoverExcluded, type MissingAlbum } from './api';
  import Cover from './Cover.svelte';
  import CoverStatus from './CoverStatus.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import PageLoading from './PageLoading.svelte';
  import Row from './Row.svelte';
  import SectionHead from './SectionHead.svelte';
  import { albumSubject, viewOver } from './panel.svelte';
  import { matchesSearch, searchNone } from './pageView.svelte';
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
  // The page's words leaving none of the list (decision 0095): said from the magnifier.
  searchNone(
    () =>
      kind === 'missing'
        ? !shownMissing.length && { n: missing?.albums.length ?? 0, one: 'record' }
        : !shownExcluded.length && !shownArtists.length && { n: (excluded?.albums.length ?? 0) + (excluded?.artists.length ?? 0), one: 'exclusion' },
    'panel',
  );
  const when = (secs: number) => new Date(secs * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });

  // A missing album opened: its record's page (C54).
  const open = (a: MissingAlbum) => viewOver(albumSubject('cratarr', a.record));

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
    <p class="note">From the lists of {missing.artists} of your artists at MusicBrainz, Discogs and Bandcamp.</p>
    <ul class="rows">
      {#each shownMissing as a (a.record)}
        <li>
          <Row size="large" title={a.title} label={`Open ${a.title}`} onclick={() => open(a)}>
            {#snippet front()}<span class="art"><Cover src={a.cover} /><CoverStatus mark={a.request ?? null} small /></span>{/snippet}
            {#snippet sub()}{a.artist}{a.year ? ` · ${a.year}` : ''}{a.kind && a.kind !== 'Album' ? ` · ${a.kind}` : ''}{/snippet}
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
</style>
