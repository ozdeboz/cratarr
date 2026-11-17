<script lang="ts">
  // A label's page under its name (W11, C56; lib/labelPage): its records by year, newest first, each with its cover, title and
  // line (artist, year, catalogue number, its type), what the library has marked, a request's mark on its cover; a tap opens its
  // record, made one at open and tied to this label (lib/openRecord). While its lists are read it asks again a little later; the
  // next hundred of each source's list is read as the list's end comes into view (OQ10 (a)). As Find's rows (Beyond's records).
  import { getLabel } from './api';
  import { openNotHad } from './albums.svelte';
  import Cover from './Cover.svelte';
  import CoverBusy from './CoverBusy.svelte';
  import CoverStatus from './CoverStatus.svelte';
  import { fromDiscogs, moreToRead, rowLine, rowOpenable, type LabelPageOf, type LabelRow } from './labelPage';
  import PageLoading from './PageLoading.svelte';
  import { matchesSearch, searchNone } from './pageView.svelte';
  import Row from './Row.svelte';
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

  const rows = $derived(searching ? page.rows.filter((r) => matchesSearch(r.title, r.artist, r.catno)) : page.rows);
  // The page's words leave none of it: said from the magnifier (decision 0095).
  searchNone(() => searching && !rows.length && { n: page.rows.length, one: 'record' }, 'panel');

  /** Asked again while its lists are read: this often, at most this many times for one label. */
  const LOOK_MS = 4000;
  const LOOK_TRIES = 8;
  let looked = { id: '', n: 0 };
  $effect(() => {
    const p = page;
    if (!p.looking) return;
    if (looked.id !== p.id) looked = { id: p.id, n: 0 };
    if (looked.n++ >= LOOK_TRIES) return;
    const t = setTimeout(
      () =>
        getLabel(p.id).then(
          (q) => page.id === p.id && onpage(q),
          () => {},
        ),
      LOOK_MS,
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

  /** The row being opened: its record made first where none holds it yet. */
  let opening = $state<number | null>(null);
  async function open(r: LabelRow, i: number) {
    if (opening !== null) return;
    opening = i;
    try {
      await openNotHad(rowOpenable(r), page.id);
    } catch (e) {
      toast.fail(e, "It couldn't be opened");
    } finally {
      opening = null;
    }
  }
</script>

{#if rows.length}
  <ul class="rows">
    {#each rows as r, i (i)}
      <li>
        <Row size="large" title={r.title} label={`Open ${r.title}`} onclick={() => open(r, i)}>
          {#snippet front()}<span class="art"><Cover src={r.cover ?? null} /><CoverStatus mark={r.request ?? null} small />{#if opening === i}<CoverBusy small />{/if}</span>{/snippet}
          {#snippet sub()}{rowLine(r)}{#if r.had}<span class="have"> · in your library</span>{/if}{/snippet}
        </Row>
      </li>
    {/each}
    <!-- The list's end: the next hundred of each source, asked again once the list has grown. -->
    {#if !searching && moreToRead(page).length}{#key page.rows.length}<li class="more" {@attach nearEnd}></li>{/key}{/if}
  </ul>
  <!-- Discogs' data shown with its attribution (its terms; 0076). -->
  {#if fromDiscogs(page.rows)}<p class="credit">Data from <a href="https://www.discogs.com" target="_blank" rel="noopener noreferrer">Discogs</a></p>{/if}
{:else if page.looking}
  <PageLoading />
{:else if !searching}
  <p class="empty">No records.</p>
{/if}

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

  .have {
    color: var(--accent-ink);
  }

  .empty {
    margin: 0 16px 12px;
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
