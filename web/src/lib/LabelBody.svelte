<script lang="ts">
  // A label's page under its name (W11, C56; lib/labelPage): what the house has apart from what it hasn't, as an artist's page
  // (F10): "In your library" at once, never moved by the lists' reading; "Not in your library" the section's loader while they
  // are read, then their records, or why there are none in F1's words. Each by year, newest first, with its cover, title and line
  // (artist, year, catalogue number, its type), a request's mark on its cover; a tap opens its record, made one at open and tied
  // to this label (lib/openRecord). While its lists are read it asks again as a record not had does; the next hundred of each
  // source's list is read as the list's end comes into view (OQ10 (a)). As Find's rows (Beyond's records).
  import { setContext } from 'svelte';
  import { getLabel } from './api';
  import { openNotHad } from './albums.svelte';
  import Cover from './Cover.svelte';
  import CoverBusy from './CoverBusy.svelte';
  import CoverStatus from './CoverStatus.svelte';
  import { HOLD } from './holdPlace.svelte';
  import { fromDiscogs, labelRows, moreToRead, rowLine, rowOpenable, type LabelPageOf, type LabelRow } from './labelPage';
  import PageLoading from './PageLoading.svelte';
  import { matchesSearch, searchNone } from './pageView.svelte';
  import { lookAgainAfter, sectionPlace } from './recordNotHad';
  import Row from './Row.svelte';
  import SectionHead from './SectionHead.svelte';
  import { toast } from './toast.svelte';

  let {
    page,
    onpage,
    searching = false,
  }: {
    page: LabelPageOf;
    /** The page read again (still looking, or reading on): the panel shows it. */
    onpage: (p: LabelPageOf) => void;
    /** The panel's own search is on: only the records that have the words, nothing read on. */
    searching?: boolean;
  } = $props();

  /** While the panel's own search is on, only the records that have the words. */
  const keep = (r: LabelRow) => !searching || matchesSearch(r.title, r.artist, r.catno);
  const had = $derived(page.had.filter(keep));
  const notHad = $derived(page.not_had.filter(keep));
  // The page's words leave none of it: said from the magnifier (decision 0095).
  searchNone(() => searching && !had.length && !notHad.length && { n: labelRows(page).length, one: 'record' }, 'panel');
  /** What Not in your library shows (F10): its records; while the lists are read, the loader naming what it reads; once read,
   *  why there are none. While the panel's search is on, its hits alone. */
  const notHadPlace = $derived(searching ? (notHad.length ? 'rows' : null) : sectionPlace(page, notHad.length));
  // Not in your library's read never holds the panel (lib/holdPlace): In your library shows at once (F10).
  setContext(HOLD, undefined);

  /** Asked again while its lists are read, as a record not had is (lib/recordNotHad), until they are. */
  let looked = { id: '', n: 0 };
  $effect(() => {
    const p = page;
    if (looked.id !== p.id || !p.looking) looked = { id: p.id, n: 0 };
    if (!p.looking) return;
    const t = setTimeout(
      () =>
        getLabel(p.id).then(
          (q) => page.id === p.id && onpage(q),
          () => {},
        ),
      lookAgainAfter(looked.n++),
    );
    return () => clearTimeout(t);
  });

  /** The next hundred of each source whose list reads on, one after another; a source that fails waits for the next look. */
  let reading = false;
  async function readOn() {
    const p = page;
    if (reading || searching || !moreToRead(p).length) return;
    reading = true;
    try {
      for (const source of moreToRead(p)) {
        const now = await getLabel(p.id, source);
        if (page.id !== p.id) return;
        onpage(now);
      }
    } catch {
      /* the server says why on its next look */
    } finally {
      reading = false;
    }
  }
  function nearEnd(el: Element) {
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && void readOn(), { rootMargin: '400px' });
    io.observe(el);
    return () => io.disconnect();
  }

  /** The row being opened (its section and place): its record made first where none holds it yet. */
  let opening = $state<string | null>(null);
  async function open(r: LabelRow, key: string) {
    if (opening !== null) return;
    opening = key;
    try {
      await openNotHad(rowOpenable(r), page.id);
    } catch (e) {
      toast.fail(e, "It couldn't be opened");
    } finally {
      opening = null;
    }
  }
</script>

{#snippet row(r: LabelRow, key: string)}
  <li>
    <Row size="large" title={r.title} label={`Open ${r.title}`} onclick={() => open(r, key)}>
      {#snippet front()}<span class="art"><Cover src={r.cover ?? null} /><CoverStatus mark={r.request ?? null} small />{#if opening === key}<CoverBusy small />{/if}</span>{/snippet}
      {#snippet sub()}{rowLine(r)}{/snippet}
    </Row>
  </li>
{/snippet}

<!-- What the house has, at once and apart (F10): never replaced or moved by the lists' reading. -->
{#if had.length}
  <section aria-label="In your library">
    <SectionHead title="In your library" />
    <ul class="rows">
      {#each had as r, i (i)}{@render row(r, `had:${i}`)}{/each}
    </ul>
  </section>
{/if}
{#if notHadPlace}
  <section aria-label="Not in your library">
    <SectionHead title="Not in your library" />
    {#if notHadPlace === 'rows'}
      <ul class="rows">
        {#each notHad as r, i (i)}{@render row(r, `not:${i}`)}{/each}
        <!-- The list's end: the next hundred of each source, asked again once the list has grown. -->
        {#if !searching && moreToRead(page).length}{#key page.not_had.length}<li class="more" {@attach nearEnd}></li>{/key}{/if}
      </ul>
    {:else if 'reading' in notHadPlace}<PageLoading><p class="opening">{notHadPlace.reading}</p></PageLoading>{:else}<p class="empty">{notHadPlace.line}</p>{/if}
  </section>
{/if}
<!-- Discogs' data shown with its attribution (its terms; 0076). -->
{#if fromDiscogs(labelRows(page))}<p class="credit">Data from <a href="https://www.discogs.com" target="_blank" rel="noopener noreferrer">Discogs</a></p>{/if}
{#if !searching && !had.length && !notHadPlace}<p class="empty">No records.</p>{/if}

<style>
  .rows {
    margin: 0 0 12px;
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

  section {
    margin-top: 16px;
  }

  .empty {
    margin: 0 16px 12px;
    color: var(--muted);
  }

  /* What a section's loader waits for, under its mark (lib/PageLoading's own line). */
  .opening {
    margin: 0;
    color: var(--muted);
  }

  .credit {
    margin: 0 16px 12px;
    color: var(--muted);
    font-size: 11px;
  }

  .credit a {
    color: inherit;
  }
</style>
