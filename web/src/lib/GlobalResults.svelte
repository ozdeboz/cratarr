<script lang="ts">
  // Global search's results (decision 0072): a group per kind but the kind the page already shows (`except`), a few
  // of each, why each is there; the server's (the library, what is not in it) and the browser's (pages, Settings'
  // parts; lib/everywhere), as lib/globalSearch holds them. Shown as the page's or the panel's Everywhere (the owner,
  // 2026-10-25, mockup 3: two tabs under the search, lib/SearchScope), or as Home's own. Nothing found anywhere: one
  // button, Find more music, which closes the search and looks the words up on MusicBrainz (the Find page).
  import { takeBackExcluded } from './api';
  import Cover from './Cover.svelte';
  import ElsewhereAlbum from './ElsewhereAlbum.svelte';
  import { globalAsked, globalGroups, globalSearch } from './globalSearch.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Row from './Row.svelte';
  import { viewOver } from './panel.svelte';
  import { pageView } from './pageView.svelte';
  import type { SearchResult } from './search';
  import { toast } from './toast.svelte';

  /** The kinds the page already shows: left out here (the browser's, Settings' parts, by their `kind` too). */
  let { except = [] }: { except?: string[] } = $props();

  const q = $derived(pageView.query.trim());
  const groups = $derived(globalGroups(except));
  const asked = $derived(globalAsked());
  const why = (m: { on: string; text: string; count?: number }[]) => {
    const t = m.find((x) => x.on !== 'name');
    return t ? `${t.on === 'track' ? 'Track' : t.on === 'album' ? 'Album' : t.on === 'artist' ? 'By' : t.on}: ${t.text}${t.count && t.count > 1 ? ` · ${t.count}` : ''}` : null;
  };

  // A tap: a page or a part where it shows (the words kept, so it narrows to them), an album not in the library on
  // Bandcamp or its menu, what Discover left out its menu (Take back), the rest in the panel.
  let elsewhere = $state<ElsewhereAlbum>();
  let leftOut = $state<SearchResult | null>(null);
  function open(it: SearchResult) {
    if (it.subject.startsWith('#')) location.hash = it.subject;
    else if (it.kind === 'missing')
      void elsewhere?.open({ release_group: it.subject.slice('missing:'.length), title: it.title, artist: it.artist ?? '', year: null, cover: it.cover ?? null });
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
        <h3>{g.label}</h3>
        <ul class="rows">
          {#each g.items as it (it.subject + (it.track ?? ''))}
            <li>
              <Row size="large" title={it.title} label={`Open ${it.title}`} onclick={() => open(it)}>
                {#snippet front()}{#if it.kind !== 'setting' && it.kind !== 'page'}<span class="art" class:round={it.kind === 'artist'}><Cover src={it.cover ?? null} /></span>{/if}{/snippet}
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
<ElsewhereAlbum bind:this={elsewhere} />
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

  h3 {
    margin: 14px 16px 6px;
    color: var(--muted);
    font-size: 14px;
    font-weight: 500;
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
