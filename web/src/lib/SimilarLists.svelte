<script lang="ts">
  // Similar playlists or crates, at a playlist's or a crate's foot (the owner, 2026-10-23: every list with a profile
  // gets its own kind's similar ones at the bottom): the closest of the account's own by the one similarity (the
  // server's), each its covers (a mosaic of four, else the first) and its name; a tap views it. Left out when there are
  // none.
  import { untrack } from 'svelte';
  import { albumCoverUrl, playlistCovers, similarCrates, similarPlaylists } from './api';
  import Cover, { mosaicOf } from './Cover.svelte';
  import { CRATE, crateArt, crateName, crates, loadCrates } from './crates.svelte';
  import Icon from './Icon.svelte';
  import { shownPlaylists } from './output.svelte';
  import { viewPlaylist } from './panel.svelte';
  import SectionHead from './SectionHead.svelte';

  let { kind, id }: { kind: 'playlist' | 'crate'; id: string } = $props();

  let ids = $state<string[]>([]);
  /** A playlist's covers, asked for as it shows (a crate's are on its card already). */
  let covers = $state<Record<string, string[]>>({});
  $effect(() => {
    const [k, at] = [kind, id];
    ids = [];
    if (k === 'crate' && !untrack(() => crates.list)) void loadCrates();
    (k === 'playlist' ? similarPlaylists(at) : similarCrates(at)).then(
      (found) => {
        if (k !== kind || at !== id) return;
        ids = found.map((f) => f.id);
        if (k === 'playlist')
          for (const p of ids)
            if (!covers[p])
              playlistCovers(p).then(
                (list) => (covers[p] = list.map((c) => albumCoverUrl(c.source, c.album_id))),
                () => {},
              );
      },
      () => {},
    );
  });
  const items = $derived(
    ids.flatMap((x) => {
      const name = kind === 'playlist' ? shownPlaylists().find((p) => p.id === x)?.name : crateName(x);
      return name ? [{ id: x, name, art: mosaicOf(kind === 'playlist' ? (covers[x] ?? []) : crateArt(x)) }] : [];
    }),
  );
</script>

{#if items.length}
  <section aria-label={kind === 'playlist' ? 'Similar playlists' : 'Similar crates'}>
    <SectionHead title={kind === 'playlist' ? 'Similar playlists' : 'Similar crates'} />
    <ul>
      {#each items as it (it.id)}
        <li>
          <button onclick={() => viewPlaylist(kind === 'playlist' ? it.id : CRATE + it.id)} aria-label="View {it.name}">
            <span class="cover" class:mosaic={it.art.length === 4}>
              {#if it.art.length}
                {#each it.art as src (src)}<Cover {src} />{/each}
              {:else}
                <span class="icon"><Icon name={kind === 'playlist' ? 'playlists' : 'crates'} size={32} /></span>
              {/if}
            </span>
            <span class="name">{it.name}</span>
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  section {
    margin-top: 24px;
  }

  ul {
    display: flex;
    gap: 12px;
    margin: 0;
    padding: 4px 16px 0;
    overflow-x: auto;
    overscroll-behavior-x: contain;
    scroll-padding-inline: 16px;
    scroll-snap-type: x proximity;
    scrollbar-width: none;
    list-style: none;
  }

  li {
    flex: none;
    width: 140px;
    scroll-snap-align: start;
  }

  button {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .cover {
    display: grid;
    width: 100%;
    aspect-ratio: 1;
    overflow: hidden;
    border-radius: 8px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .cover.mosaic {
    grid-template-columns: 1fr 1fr;
  }

  .icon {
    display: grid;
    place-items: center;
    color: var(--muted);
  }

  .name {
    width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
