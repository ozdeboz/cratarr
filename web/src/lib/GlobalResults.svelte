<script lang="ts">
  // Global search's results (decision 0072): a group per kind but the kind the page already shows (`except`), a few
  // of each, why each is there; the server's (the library, what is not in it) and the browser's (pages, Settings'
  // parts; lib/everywhere), as lib/globalSearch holds them. Shown as the page's or the panel's Everywhere (the owner,
  // 2026-10-25: the globe beside the search, lib/TopPill), or as Home's own. Nothing found anywhere: one
  // button, Find more music, which closes the search and looks the words up on MusicBrainz (the Find page).
  import { takeBackExcluded } from './api';
  import Cover from './Cover.svelte';
  import ElsewhereAlbum from './ElsewhereAlbum.svelte';
  import { globalAsked, globalGroups, globalSearch, leaveSearch, rememberSearch } from './globalSearch.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Row from './Row.svelte';
  import SectionHead from './SectionHead.svelte';
  import { artistSubject, viewOver } from './panel.svelte';
  import { libraryDot } from './albums.svelte';
  import type { FoundGroup } from './globalSearch.svelte';
  import { pageView } from './pageView.svelte';
  import type { SearchResult } from './search';
  import { toast } from './toast.svelte';

  /** The kinds the page already shows: left out here (the browser's, Settings' parts, by their `kind` too). */
  let { except = [] }: { except?: string[] } = $props();

  const q = $derived(pageView.query.trim());
  const groups = $derived(globalGroups(except));
  const asked = $derived(globalAsked());
  /** Where a group's heading leads (its ›; the owner, 2026-10-25: "Tracks (N)" and an arrow to the artist's tracks):
   *  a group found all by one artist's name, to that artist's tracks or page. */
  function moreOf(g: FoundGroup): (() => void) | undefined {
    if (g.kind !== 'track' && g.kind !== 'album') return undefined;
    const by = g.items.map((it) => it.matched.find((m) => m.on === 'artist')?.text);
    const one = by[0];
    if (!one || by.some((a) => a !== one)) return undefined;
    return () => {
      rememberSearch();
      pageView.query = '';
      viewOver(artistSubject('cratarr', one, g.kind === 'track'));
    };
  }
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
  const deep = (it: SearchResult) => DEEP_KINDS.includes(it.kind) && it.matched.some((m) => m.on !== 'name');
  function open(it: SearchResult) {
    leaveSearch(deep(it));
    if (it.subject.startsWith('#')) location.hash = it.subject;
    else if (it.kind === 'missing')
      void elsewhere?.open({ release_group: it.subject.slice('missing:'.length), title: it.title, artist: it.artist ?? '', year: null, cover: it.cover ?? null, bandcamp: it.bandcamp });
    else if (it.kind === 'excluded') leftOut = it;
    else viewOver(it.subject);
  }
  async function takeBack(it: SearchResult) {
    leftOut = null;
    try {
      await takeBackExcluded(it.subject.startsWith('excluded-artist:') ? { artist: it.subject.slice('excluded-artist:'.length) } : { key: it.subject.slice('excluded:'.length) });
      globalSearch.again++;
    } catch (e) {
      toast.fail(e, 'Could not take it back');
    }
  }

  /** Nothing anywhere: the search closed, the words looked up on MusicBrainz. */
  function findMore() {
    const words = q;
    pageView.query = '';
    location.hash = `#/find?q=${encodeURIComponent(words)}`;
  }
</script>

{#if q && (groups.length || asked)}
  <section class="global" aria-label="Everywhere">
    {#if groups.length}
      {#each groups as g (g.kind)}
        <SectionHead title={`${g.label} (${g.total.toLocaleString()})`} onmore={moreOf(g)} />
        <ul class="rows">
          {#each g.items as it (it.subject + (it.track ?? ''))}
            <li>
              <Row size="large" title={it.title} label={`Open ${it.title}`} onclick={() => open(it)}>
                {#snippet front()}{#if it.kind !== 'setting' && !it.kind.startsWith('page:')}<span class="art" class:round={it.kind === 'artist'}><Cover src={it.cover ?? null} /></span>{/if}{/snippet}
                {#snippet action()}{#if it.kind === 'missing' && finding === it.subject.slice('missing:'.length)}<button class="spin" aria-busy="true" disabled aria-label="Looking on Bandcamp"></button>{/if}{/snippet}
                {#snippet badges()}{@const dot = dotOf(it)}{#if dot}<i class="dot" style:background={dot.colour} role="img" aria-label={dot.name}></i>{/if}{/snippet}
                {#snippet sub()}{it.sub ?? ''}{#if why(it.matched)}<span class="why">{it.sub ? ' · ' : ''}{why(it.matched)}</span>{/if}{/snippet}
              </Row>
            </li>
          {/each}
        </ul>
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
  .global {
    margin-top: 4px;
  }

  .rows {
    margin: 0;
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

  .art.round {
    border-radius: 50%;
  }

  /* Looking on Bandcamp (the owner, 2026-10-25): the app's busy ring (app.css), in the row's end. */
  .spin {
    position: relative;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    background: none;
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
