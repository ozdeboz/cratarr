<script lang="ts">
  // Global search's results (decision 0072): a group per kind but the kind the page already shows (`except`), a few
  // of each, why each is there; the server's (the library, what is not in it) and the browser's (pages, Settings'
  // parts; lib/everywhere), as lib/globalSearch holds them. Shown as the page's or the panel's Everywhere (the owner,
  // 2026-10-25: the globe beside the search, lib/TopPill), or as Home's own. Three tiers (decision 0076): the library,
  // what is known elsewhere, Beyond (MusicBrainz, and Discogs with 0.9.13's server) asked once the words settle; each
  // group's heading opens it whole (Show all), Beyond's the Find page. Nothing anywhere: Find more music.
  import { includeArtist, takeBackExcluded } from './api';
  import { loadExcludedArtists } from './excludedArtists.svelte';
  const EVERYWHERE = 'excluded-everywhere:';
  import Cover from './Cover.svelte';
  import CoverStatus from './CoverStatus.svelte';
  import CoverBusy from './CoverBusy.svelte';
  import ElsewhereAlbum from './ElsewhereAlbum.svelte';
  import { beyondAlbum, globalAsked, globalGroups, globalSearch, leaveSearch, loadAll, rememberSearch, showAll } from './globalSearch.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Row from './Row.svelte';
  import SectionHead from './SectionHead.svelte';
  import { viewOver } from './panel.svelte';
  import { libraryDot } from './albums.svelte';
  import { pageView } from './pageView.svelte';
  import { leaveOnArrival } from './navStep.svelte';
  import type { SearchResult } from './search';
  import { toast } from './toast.svelte';

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
  function open(it: SearchResult) {
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
  <li>
    <Row size="large" title={it.title} label={`Open ${it.title}`} onclick={() => open(it)}>
      {#snippet front()}{#if it.kind !== 'setting' && !it.kind.startsWith('page:')}<span class="art" class:round={it.kind === 'artist'}><Cover src={it.cover ?? null} /><CoverStatus mark={it.request} small />{#if (it.kind === 'missing' && finding === it.subject.slice('missing:'.length)) || (it.kind === 'beyond' && finding === it.subject.slice('beyond:'.length))}<CoverBusy small />{/if}</span>{/if}{/snippet}
      {#snippet badges()}{@const dot = dotOf(it)}{#if dot}<i class="dot" style:background={dot.colour} role="img" aria-label={dot.name}></i>{/if}{/snippet}
      {#snippet sub()}{it.sub ?? ''}{#if it.composers?.length}<span class="why">{it.sub ? ' · ' : ''}{it.composers.map((c) => c.name).join(', ')}</span>{/if}{#if it.known?.length}<span class="why">{it.sub ? ' · ' : ''}{it.known.join(' · ')}</span>{:else if why(it.matched)}<span class="why">{it.sub ? ' · ' : ''}{why(it.matched)}</span>{/if}{/snippet}
    </Row>
  </li>
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
{#if leftOut}
  {@const it = leftOut}
  <MenuSheet label={it.title} title={it.title} images={it.cover ? [it.cover] : []} chips={[it.sub ?? '']} onclose={() => (leftOut = null)}>
    <MenuEntry icon="undo" label="Take back" onclick={() => takeBack(it)} />
  </MenuSheet>
{/if}
<!-- A server that cannot search (before 0.9.3) or did not answer: nothing more than the page's own. -->

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
