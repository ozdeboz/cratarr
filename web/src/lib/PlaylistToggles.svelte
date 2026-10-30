<script lang="ts" generics="T extends { id: string; name: string; count: number; mine?: boolean }">
  // A menu's playlists as toggle cards (Add to playlist: a track's, an album's, an artist's), with the standard size
  // buttons (lib/SizeControl, decided 2026-10-16: "so that on mobile we can get two columns"; a button that would
  // change nothing is dimmed) and the lists' header (lib/ListHead: by name, by size, or those it is in ticked first,
  // either way round). The cards sit in columns, as many as the size gives (LIST_WIDTHS: never three on a phone); a
  // name wraps to two lines. Size and order are remembered (prefs.listZoom, listOrder), apart from the Playlists page's.
  // One's own playlists only: another user's shared one is theirs to change (decision 0051).
  import type { Snippet } from 'svelte';
  import ListHead from './ListHead.svelte';
  import { howMany, LIST_ORDERS, LIST_WIDTHS, playlistBy } from './pageView.svelte';
  import { prefs } from './prefs.svelte';
  import SizeControl from './SizeControl.svelte';

  let {
    title,
    playlists,
    ticked,
    entry,
  }: {
    title: string;
    playlists: readonly T[];
    /** It holds the item (its card's tick): what Ticked first puts first. */
    ticked: (p: T) => boolean;
    entry: Snippet<[T]>;
  } = $props();

  let width = $state(0);
  const sorted = $derived.by(() => {
    const by = playlistBy<T>(prefs.listOrder, ticked);
    const turn = prefs.listDown ? -1 : 1;
    return playlists.filter((p) => p.mine !== false).sort((a, b) => turn * by(a, b));
  });
</script>

<div class="head">
  <span class="heading">{title}</span>
  <SizeControl plain widths={LIST_WIDTHS} {width} bind:value={prefs.listZoom} />
</div>
<ListHead fields={LIST_ORDERS} bind:value={prefs.listOrder} bind:down={prefs.listDown} count={howMany(sorted.length, 'playlist')} />
<div class="grid" role="group" aria-label="Playlists" bind:clientWidth={width} style:--least="{LIST_WIDTHS[prefs.listZoom]}px">
  {#each sorted as p (p.id)}{@render entry(p)}{/each}
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 8px 0 16px;
  }

  .heading {
    min-width: 0;
    font-size: 18px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, var(--least)), 1fr));
    gap: 8px;
    padding: 4px 12px;
  }

  .grid > :global(.toggle) {
    width: 100%;
    height: 100%;
    margin: 0;
  }

  .grid > :global(.toggle span) {
    display: -webkit-box;
    white-space: normal;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
</style>
