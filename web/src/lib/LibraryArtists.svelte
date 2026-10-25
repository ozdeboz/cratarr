<script lang="ts">
  // The Library's Artists (the owner, 2026-10-25: "in the same style as the album list with a beautiful table"): the
  // libraries' albums by their artists, with the albums' header (its order, how many, Select's room) and their two
  // layouts, rows or the table (the Library's View); in the table their columns as wide as the list allows (decision
  // 0042). The page's search filters them by name. A tap views the artist in the panel, as an album's artist does.
  import { albumCoverUrl, type ShelfSource } from './api';
  import { fit } from './columns.svelte';
  import Cover from './Cover.svelte';
  import { plural } from './insights.svelte';
  import ListHead from './ListHead.svelte';
  import { artistSubject, panel, viewPlaylist } from './panel.svelte';
  import { ARTIST_ORDERS, howMany, matchesSearch } from './pageView.svelte';
  import { prefs } from './prefs.svelte';
  import Row, { type Cell } from './Row.svelte';
  import { libraryShown } from './sources.svelte';

  let { sources }: { sources: ShelfSource[] } = $props();

  type Artist = { source: string; name: string; albums: number; tracks: number; cover: string };
  const artists = $derived.by(() => {
    const by = new Map<string, Artist>();
    for (const s of sources)
      for (const a of s.albums) {
        if (a.library && !libraryShown(s.id, a.library)) continue;
        for (const name of a.artists?.length ? a.artists : a.artist ? [a.artist] : []) {
          const key = `${s.id}\u0000${name.toLocaleLowerCase()}`;
          const x = by.get(key) ?? { source: s.id, name, albums: 0, tracks: 0, cover: albumCoverUrl(s.id, a.id) };
          x.albums++;
          x.tracks += a.tracks;
          by.set(key, x);
        }
      }
    return [...by.values()];
  });
  const byName = (x: Artist, y: Artist) => x.name.localeCompare(y.name, undefined, { sensitivity: 'base', numeric: true });
  // As every search reads (lib/search, decision 0072); in the header's order, either way round.
  const shown = $derived.by(() => {
    const o = prefs.libraryArtistOrder;
    const list = artists.filter((a) => matchesSearch(a.name));
    list.sort((x, y) => (o === 'name' ? byName(x, y) : x[o] - y[o] || byName(x, y)));
    return prefs.libraryArtistDown ? list.reverse() : list;
  });

  const table = $derived(prefs.albumLayout === 'table');
  let width = $state(0);
  const cols = $derived(table ? fit('artists', width) : []);
  const cellsOf = (a: Artist): Cell[] => cols.map((c) => ({ ...c, text: (c.id === 'albums' ? a.albums : a.tracks).toLocaleString() }));
  // The column names, a tap on one ordering by it (most first), again the other way round.
  const headCells = $derived<Cell[]>(
    cols.map((c) => {
      const o = c.id as 'albums' | 'tracks';
      return {
        ...c,
        text: c.label,
        on: prefs.libraryArtistOrder === o,
        onclick: () => {
          if (prefs.libraryArtistOrder === o) prefs.libraryArtistDown = !prefs.libraryArtistDown;
          else ((prefs.libraryArtistOrder = o), (prefs.libraryArtistDown = true));
        },
      };
    }),
  );
</script>

<div class="lists" class:table bind:clientWidth={width}>
  {#if !shown.length}
    <p class="empty">{artists.length ? 'No artists match.' : 'No artists.'}</p>
  {:else}
    <ListHead fields={ARTIST_ORDERS} bind:value={prefs.libraryArtistOrder} bind:down={prefs.libraryArtistDown} count={howMany(shown.length, 'artist')} />
    {#if cols.length}
      <Row size="large" as="div" head title="Name" cells={headCells}>
        {#snippet front()}<span class="face-space"></span>{/snippet}
      </Row>
    {/if}
    <ul class:table>
      {#each shown as a (a.source + a.name)}
        <li>
          <Row
            size={table ? 'large' : 'xl'}
            title={a.name}
            sub={table && cols.length ? undefined : `${plural(a.albums, 'album')} · ${plural(a.tracks, 'track')}`}
            cells={cols.length ? cellsOf(a) : undefined}
            label={`View ${a.name}`}
            active={panel.viewed === artistSubject(a.source, a.name)}
            onclick={() => viewPlaylist(artistSubject(a.source, a.name))}
          >
            {#snippet front()}<span class="face"><Cover src={a.cover} /></span>{/snippet}
          </Row>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  ul {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* As the albums' rows: drawn as they come near (a long list). */
  li {
    min-width: 0;
    content-visibility: auto;
    contain-intrinsic-size: auto 80px;
  }

  /* The table (decision 0042): one artist a line, the lines touching, a smaller face in front. */
  ul.table {
    display: block;
  }

  ul.table li {
    contain-intrinsic-size: auto 60px;
  }

  .face {
    display: block;
    width: 64px;
    height: 64px;
    overflow: hidden;
    border-radius: 50%;
  }

  .table .face,
  .face-space {
    width: 48px;
    height: 48px;
    margin-right: 6px;
  }

  .face-space {
    display: block;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }
</style>
