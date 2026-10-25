<script lang="ts">
  // Global search, at the end of what a page's or a panel's own search leaves (decision 0072; the owner, 2026-10-25:
  // "after filtering the current page, at the end of what remains should be our global search results"): the same
  // words asked of the server (/api/search), a group per kind but the kind the page already shows (`except`), a few of
  // each, why each is there. Nothing found anywhere: one button, Find more music, which closes the search and looks the
  // words up on MusicBrainz (the Find page).
  import { search, type SearchKind, type ServerSearch } from './api';
  import Cover from './Cover.svelte';
  import Row from './Row.svelte';
  import SectionHead from './SectionHead.svelte';
  import { viewOver } from './panel.svelte';
  import { pageView } from './pageView.svelte';
  import { words } from './search';

  let { except = [] }: { except?: SearchKind[] } = $props();

  const KINDS: { kind: SearchKind; label: string }[] = [
    { kind: 'artist', label: 'Artists' },
    { kind: 'album', label: 'Albums' },
    { kind: 'track', label: 'Tracks' },
    { kind: 'playlist', label: 'Playlists' },
    { kind: 'crate', label: 'Crates' },
    { kind: 'genre', label: 'Genres' },
  ];
  /** A few of each kind. */
  const EACH = 5;

  const q = $derived(pageView.query.trim());
  let found = $state<{ q: string; r: ServerSearch } | null>(null);
  // Asked a moment after the words stop changing; nothing for no words.
  $effect(() => {
    const now = q;
    found = null;
    if (!words(now).length) return;
    let gone = false;
    const t = setTimeout(() => {
      search(now, KINDS.map((k) => k.kind).filter((k) => !except.includes(k)), EACH).then(
        (r) => !gone && (found = { q: now, r }),
        () => {},
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  const groups = $derived(
    found ? KINDS.filter((k) => !except.includes(k.kind)).map((k) => ({ ...k, items: found!.r.groups[k.kind]?.items ?? [] })).filter((g) => g.items.length) : [],
  );
  const why = (m: { on: string; text: string; count?: number }[]) => {
    const t = m.find((x) => x.on !== 'name');
    return t ? `${t.on === 'track' ? 'Track' : t.on === 'album' ? 'Album' : t.on === 'artist' ? 'By' : t.on}: ${t.text}${t.count && t.count > 1 ? ` · ${t.count}` : ''}` : null;
  };

  /** Nothing anywhere: the search closed, the words looked up on MusicBrainz. */
  function findMore() {
    const words = q;
    pageView.query = '';
    location.hash = `#/find?q=${encodeURIComponent(words)}`;
  }
</script>

{#if q && found && found.q === q}
  <section class="global" aria-label="Everywhere">
    {#if groups.length}
      <SectionHead title="Everywhere" />
      {#each groups as g (g.kind)}
        <h3>{g.label}</h3>
        <ul class="rows">
          {#each g.items as it (it.subject + (it.track ?? ''))}
            <li>
              <Row size="large" title={it.title} label={`Open ${it.title}`} onclick={() => viewOver(it.subject)}>
                {#snippet front()}<span class="art" class:round={it.kind === 'artist'}><Cover src={it.cover ?? null} /></span>{/snippet}
                {#snippet sub()}{it.sub ?? ''}{#if why(it.matched)}<span class="why">{it.sub ? ' · ' : ''}{why(it.matched)}</span>{/if}{/snippet}
              </Row>
            </li>
          {/each}
        </ul>
      {/each}
    {:else}
      <div class="none">
        <p>Nothing in Cratarr for “{q}”.</p>
        <button class="find" onclick={findMore}>Find more music</button>
      </div>
    {/if}
  </section>
{/if}
<!-- A server that cannot search (before 0.9.3) or did not answer: nothing more than the page's own. -->

<style>
  .global {
    margin-top: 24px;
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
