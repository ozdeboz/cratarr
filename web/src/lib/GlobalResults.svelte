<script lang="ts">
  // Global search's results (decision 0072): a group per kind but the kind the page already shows (`except`), a few
  // of each, why each is there; the server's (the library, what is not in it) and the browser's (pages, Settings'
  // parts; lib/everywhere), as lib/globalSearch holds them. Shown as the page's or the panel's Everywhere (the owner,
  // 2026-10-25: the globe beside the search, lib/TopPill), or as Home's own. Three tiers (decision 0076): the library,
  // what is known elsewhere, Beyond (MusicBrainz, and Discogs with 0.9.13's server) asked once the words settle; each
  // group's heading opens it whole (Show all), Beyond's the Find page. Nothing anywhere: Find more music. First, where
  // the server makes them (0.11), Grab a crate (the owner, 2026-10-30; was Make a crate of this): Crates, the words read into a quick crate's sentence (lib/QuickCrate);
  // where the words read as a crate of two kinds or more, that crate as a card (lib/SearchCrateCard, mockup 1a). A
  // result's long-press, right-click or ⋮ opens its own menu, as the same thing's does in any list (decision 0058).
  import { fromAction } from 'svelte/attachments';
  import { includeArtist, takeBackExcluded } from './api';
  import { longpress } from './longpress';
  import WorkMenu from './WorkMenu.svelte';
  import WorkSheet from './WorkSheet.svelte';
  import { loadExcludedArtists } from './excludedArtists.svelte';
  const EVERYWHERE = 'excluded-everywhere:';
  import Cover from './Cover.svelte';
  import CoverStatus from './CoverStatus.svelte';
  import CoverBusy from './CoverBusy.svelte';
  import ElsewhereAlbum from './ElsewhereAlbum.svelte';
  import Icon from './Icon.svelte';
  import { beyondAlbum, globalAsked, globalGroups, globalSearch, leaveSearch, loadAll, rememberSearch, showAll } from './globalSearch.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Row from './Row.svelte';
  import SectionHead from './SectionHead.svelte';
  import { viewOver } from './panel.svelte';
  import { libraryDot } from './albums.svelte';
  import { pageView } from './pageView.svelte';
  import { openQuickCrate, quickOn } from './quickCrates.svelte';
  import { leaveOnArrival } from './navStep.svelte';
  import type { SearchResult } from './search';
  import { toast } from './toast.svelte';
  import AlbumMenu from './AlbumMenu.svelte';
  import ArtistRowMenu from './ArtistRowMenu.svelte';
  import SearchCrateCard from './SearchCrateCard.svelte';
  import SubjectMenu from './SubjectMenu.svelte';
  import TrackMenu from './TrackMenu.svelte';

  /** The kinds the page already shows: left out here (the browser's, Settings' parts, by their `kind` too). */
  let { except = [] }: { except?: string[] } = $props();

  const q = $derived(pageView.query.trim());
  const groups = $derived(globalGroups(except));
  const asked = $derived(globalAsked());
  // An album's or a track's library, as a dot before its title, as the Library's lists show it (0.9.5): from its
  // album's subject (`album:<source>/<id>`), the Library's list knowing which library each is in.
  function dotOf(it: SearchResult) {
    if (it.kind !== 'album' && it.kind !== 'track') return null;
    const m = /^album:([^/]+)\/(.+)$/.exec(it.subject);
    return m ? libraryDot(m[1], { id: m[2] }) : null;
  }
  const why = (m: { on: string; text: string; count?: number }[]) => {
    const t = m.find((x) => x.on !== 'name');
    return t ? `${t.on === 'track' ? 'Track' : t.on === 'album' ? 'Album' : t.on === 'artist' ? 'By' : t.on}: ${t.text}${t.count && t.count > 1 ? ` · ${t.count}` : ''}` : null;
  };

  // A tap: a page or a part where it shows (the words kept, so it narrows to them), an album not in the library on
  // Bandcamp or its menu, what Discover left out its menu (Take back), the rest in the panel.
  let elsewhere = $state<ElsewhereAlbum>();
  let finding = $state<string | null>(null);
  let leftOut = $state<SearchResult | null>(null);
  // A result tapped empties the search (the owner, 2026-10-25: it stayed in the field over the panel it opened); a set
  // found deep keeps the words, so it opens narrowed to them as the page's own deep find does (decision 0072), its own
  // results, not everywhere's, in the panel.
  const DEEP_KINDS = ['album', 'artist', 'playlist', 'crate'];
  const KNOWN = ['spotify', 'suggested', 'requested'];
  const deep = (it: SearchResult) => DEEP_KINDS.includes(it.kind) && it.matched.some((m) => m.on !== 'name');
  /** A work found (0.12): its sheet over the results, which stay; its menu (a long-press, a right-click or ⋮). */
  let workOpen = $state<{ mbid: string; title: string } | null>(null);
  let workMenu = $state<{ mbid: string; title: string } | null>(null);
  const workOf = (it: SearchResult) => ({ mbid: it.subject.slice('work:'.length), title: it.title });
  function open(it: SearchResult) {
    if (it.kind === 'work') return void (workOpen = workOf(it));
    leaveSearch(deep(it));
    if (it.subject.startsWith('#')) location.hash = it.subject;
    else if (it.kind === 'missing')
      void elsewhere?.open({ release_group: it.subject.slice('missing:'.length), title: it.title, artist: it.artist ?? '', year: null, cover: it.cover ?? null, bandcamp: it.bandcamp });
    else if (it.kind === 'excluded') leftOut = it;
    else if (KNOWN.includes(it.kind)) {
      // Known elsewhere: opened as a missing album is, by its release group where its subject carries one.
      const rg = /^[a-z]+:([0-9a-f-]{36})$/.exec(it.subject)?.[1] ?? null;
      void elsewhere?.open({ release_group: rg, title: it.title, artist: it.artist ?? '', year: null, cover: it.cover ?? null, bandcamp: it.bandcamp });
    }
    else if (it.kind === 'beyond') {
      const a = beyondAlbum(it.subject);
      if (a) void elsewhere?.open({ release_group: a.release_group ?? null, title: a.title, artist: a.artist, year: a.year, cover: a.cover, bandcamp: a.bandcamp ?? null, discogs_url: a.discogs_url ?? null });
    }
    else viewOver(it.subject);
  }
  async function takeBack(it: SearchResult) {
    leftOut = null;
    try {
      // An artist excluded everywhere (0.10, decision 0083), or Discover's own left out (an artist, an album).
      if (it.subject.startsWith(EVERYWHERE)) await includeArtist(it.subject.slice(EVERYWHERE.length)).then(() => loadExcludedArtists(true));
      else await takeBackExcluded(it.subject.startsWith('excluded-artist:') ? { artist: it.subject.slice('excluded-artist:'.length) } : { key: it.subject.slice('excluded:'.length) });
      globalSearch.again++;
    } catch (e) {
      toast.fail(e, 'Could not take it back');
    }
  }

  // A result's own menu (decision 0058): a track's, an album's, an artist's, a crate's, a genre's or another word's, a
  // playlist's; the rest (a page, Settings' parts, an album not in the library, Beyond's) have none.
  type Menu =
    | { kind: 'track'; track: import('./TrackMenu.svelte').MenuTrack }
    | { kind: 'album'; source: string; id: string; title: string; artist: string | null }
    | { kind: 'artist'; source: string; name: string; cover: string | null }
    | { kind: 'subject'; subject: string };
  let menu = $state<Menu | null>(null);
  function menuOf(it: SearchResult & { track?: string }): Menu | null {
    const album = /^album:([^/]+)\/(.+)$/.exec(it.subject);
    // A track's artist, album and length as the server gives them apart from its line (0.12).
    if (it.kind === 'track' && it.track && album) {
      return {
        kind: 'track',
        track: { source: album[1], id: it.track, name: it.title, artist: it.artist ?? null, album: it.album ?? null, album_id: it.album_id ?? album[2], duration_secs: it.duration_secs ?? null },
      };
    }
    if (it.kind === 'album' && album) return { kind: 'album', source: album[1], id: album[2], title: it.title, artist: (it.sub ?? '').split(' · ')[0] || null };
    const artist = /^artist:([^/]+)\/(.+)$/.exec(it.subject);
    if (it.kind === 'artist' && artist) return { kind: 'artist', source: artist[1], name: artist[2], cover: it.cover ?? null };
    if (it.subject.startsWith('crate:')) return { kind: 'subject', subject: it.subject };
    if (it.subject.startsWith('playlist:')) return { kind: 'subject', subject: it.subject.slice('playlist:'.length) };
    return null;
  }
  const press = (it: SearchResult & { track?: string }) => fromAction(longpress, () => () => (menu = menuOf(it)));

  /** Show all's end in sight: its next page. */
  function nearEnd(more: () => void) {
    return (el: Element) => {
      const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && more(), { rootMargin: '400px' });
      io.observe(el);
      return () => io.disconnect();
    };
  }
  /** Nothing anywhere: the search closed, the words looked up on MusicBrainz. */
  function findMore() {
    const words = q;
    // Emptied once Find shows (lib/navStep), not before.
    leaveOnArrival({ search: 'clear' });
    location.hash = `#/find?q=${encodeURIComponent(words)}`;
  }
</script>

{#snippet found(it: SearchResult & { track?: string })}
  {@const has = menuOf(it) !== null}
  <li>
    {#if it.kind === 'work'}{@render work(it)}{:else}
    <Row size="large" title={it.title} label={`Open ${it.title}`} onclick={() => open(it)} attach={has ? press(it) : undefined}>
      {#snippet front()}{#if it.kind !== 'setting' && !it.kind.startsWith('page:')}<span class="art" class:round={it.kind === 'artist'}><Cover src={it.cover ?? null} /><CoverStatus mark={it.request} small />{#if (it.kind === 'missing' && finding === it.subject.slice('missing:'.length)) || (it.kind === 'beyond' && finding === it.subject.slice('beyond:'.length))}<CoverBusy small />{/if}</span>{/if}{/snippet}
      {#snippet badges()}{@const dot = dotOf(it)}{#if dot}<i class="dot" style:background={dot.colour} role="img" aria-label={dot.name}></i>{/if}{/snippet}
      {#snippet sub()}{it.sub ?? ''}{#if it.composers?.length}<span class="why">{it.sub ? ' · ' : ''}{it.composers.map((c) => c.name).join(', ')}</span>{/if}{#if it.known?.length}<span class="why">{it.sub ? ' · ' : ''}{it.known.join(' · ')}</span>{:else if why(it.matched)}<span class="why">{it.sub ? ' · ' : ''}{why(it.matched)}</span>{/if}{/snippet}
      {#snippet action()}{#if has}<button class="icon" aria-label={`Menu of ${it.title}`} title="More" onclick={() => (menu = menuOf(it))}><Icon name="moreVert" size={22} /></button>{/if}{/snippet}
    </Row>
    {/if}
  </li>
{/snippet}

<!-- A work found (0.12): its composers as its second line, then its catalogue and recordings; a tap opens it, a
     long-press, a right-click or ⋮ its menu (lib/WorkMenu). -->
{#snippet work(it: SearchResult)}
  <Row size="large" title={it.title} label={`Open ${it.title}`} onclick={() => open(it)} attach={fromAction(longpress, () => () => (workMenu = workOf(it)))}>
    {#snippet front()}<span class="art"><Cover src={it.cover ?? null} /></span>{/snippet}
    {#snippet sub()}{(it.composers ?? []).map((c) => c.name).join(', ')}{#if it.sub}<span class="why">{it.composers?.length ? ' · ' : ''}{it.sub}</span>{/if}{/snippet}
    {#snippet action()}<button class="tool" aria-label="{it.title}: its menu" onclick={() => (workMenu = workOf(it))}><Icon name="moreVert" size={22} /></button>{/snippet}
  </Row>
{/snippet}

{#if q && globalSearch.all}
  <!-- Show all (decision 0076): one kind whole, more as it scrolls; back returns to every group. -->
  {@const kind = globalSearch.all}
  {@const all = globalSearch.allFound}
  {@const label = groups.find((g) => g.kind === kind)?.label ?? ''}
  <section class="global" aria-label={label}>
    <SectionHead title={all ? `${label} (${all.total.toLocaleString()})` : label} onback={() => history.back()} />
    <ul class="rows">
      {#each all?.items ?? [] as it (it.subject + (it.track ?? ''))}{@render found(it)}{/each}
      {#if all && !all.done}<li class="more" {@attach nearEnd(() => loadAll(all.q, all.kind))}></li>{/if}
    </ul>
  </section>
{:else if q && (groups.length || asked)}
  <section class="global" aria-label="Everywhere">
    {#if quickOn()}
      <SearchCrateCard {q}>
        {#snippet plain()}
          <ul class="rows">
            <li>
              <Row size="large" title="Grab a crate" sub={q} label={`Grab a crate of ${q}`} onclick={() => openQuickCrate(q)}>
                {#snippet front()}<span class="art made"><Icon name="crates" size={24} /></span>{/snippet}
              </Row>
            </li>
          </ul>
        {/snippet}
      </SearchCrateCard>
    {/if}
    {#if groups.length}
      {#each groups as g (g.kind)}
        <!-- Its heading opens it whole (decision 0076: every group's results, shown all); Beyond's, Find more music. -->
        <SectionHead title={`${g.label} (${g.total.toLocaleString()})`} onmore={g.kind === 'beyond' ? findMore : () => showAll(g.kind)} />
        <ul class="rows">
          {#each g.items as it (it.subject + (it.track ?? ''))}{@render found(it)}{/each}
        </ul>
        <!-- Discogs' data shown with its attribution (its terms; 0076). -->
        {#if g.kind === 'beyond' && globalSearch.beyond?.albums.some((a) => a.sources.includes('discogs'))}
          <p class="credit">Data from <a href="https://www.discogs.com" target="_blank" rel="noopener noreferrer">Discogs</a></p>
        {/if}
      {/each}
    {:else if asked}
      <div class="none">
        <p>Nothing in Cratarr for “{q}”.</p>
        <button class="find" onclick={findMore}>Find more music</button>
      </div>
    {/if}
  </section>
{/if}
<ElsewhereAlbum bind:this={elsewhere} bind:finding />
{#if menu?.kind === 'track'}
  <TrackMenu track={menu.track} onclose={() => (menu = null)} />
{:else if menu?.kind === 'album'}
  <AlbumMenu source={menu.source} album={{ id: menu.id, title: menu.title, artist: menu.artist }} onclose={() => (menu = null)} />
{:else if menu?.kind === 'artist'}
  <ArtistRowMenu source={menu.source} name={menu.name} cover={menu.cover} onclose={() => (menu = null)} />
{:else if menu?.kind === 'subject'}
  <SubjectMenu subject={menu.subject} onclose={() => (menu = null)} />
{/if}
{#if leftOut}
  {@const it = leftOut}
  <MenuSheet label={it.title} title={it.title} images={it.cover ? [it.cover] : []} chips={[it.sub ?? '']} onclose={() => (leftOut = null)}>
    <MenuEntry icon="undo" label="Take back" onclick={() => takeBack(it)} />
  </MenuSheet>
{/if}
<!-- A server that cannot search (before 0.9.3) or did not answer: nothing more than the page's own. -->

{#if workMenu}{@const w = workMenu}<WorkMenu mbid={w.mbid} title={w.title} onopen={() => (workOpen = w)} onclose={() => (workMenu = null)} />{/if}
{#if workOpen}<WorkSheet mbid={workOpen.mbid} title={workOpen.title} onclose={() => (workOpen = null)} />{/if}

<style>
  /* The pages' own side gutter (the owner: too little room on the left), outside any page's <main>. */
  .global {
    margin-top: 4px;
    padding: 0 clamp(12px, 2vw, 32px);
  }

  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* A work's ⋮ (0.12), as WorkSheet's. */
  .tool {
    display: grid;
    place-items: center;
    width: 40px;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .art {
    position: relative;
    display: block;
    width: 48px;
    height: 48px;
    overflow: hidden;
    border-radius: 6px;
  }

  .art.round {
    border-radius: 50%;
  }

  /* Grab a crate: the crates' icon where a cover would be. */
  .art.made {
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent-ink);
  }

  /* A result's ⋮ (its menu), as a list row's. */
  .icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  /* The library's dot, as the Library's lists have it. */
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .why {
    color: var(--accent-ink);
  }

  .credit {
    margin: 0 0 8px 16px;
    color: var(--muted);
    font-size: 11px;
  }

  .credit a {
    color: inherit;
  }

  .none {
    display: grid;
    justify-items: center;
    gap: 12px;
    padding: 24px 16px;
    color: var(--muted);
    text-align: center;
  }

  .none p {
    margin: 0;
  }

  /* The accent's button, as Discover's Play on Bandcamp. */
  .find {
    padding: 10px 22px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

</style>
