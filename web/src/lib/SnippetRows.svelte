<script lang="ts">
  // A crate's tracks as rows with their snippets (0.12, the owner: "as buttons which when pressed expand a waveform as
  // A2"): its cover, name, artist and length, its round button playing the track's liveliest 20 s and opening the row
  // on its waveform (lib/SnippetStrip); again, it stops and folds. One row open at a time; `key` changed, or leaving,
  // ends it. The crate search's list (lib/CrateResults) and a record's sheet (lib/RecordSheet); `onplay`: a tap plays
  // from the track, `onmenu`: its ⋮ (and a long-press) its menu.
  import { onDestroy, untrack } from 'svelte';
  import { fromAction } from 'svelte/attachments';
  import { coverUrl, type PlaylistRow } from './api';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import { clockText } from './remote.svelte';
  import Row from './Row.svelte';
  import SnippetMark from './SnippetMark.svelte';
  import SnippetStrip from './SnippetStrip.svelte';
  import { endSnippet, heardShare, snippet, startSnippet, type SnippetTrack } from './snippetPlayer.svelte';
  import TrackCover from './TrackCover.svelte';

  let {
    source,
    items,
    key = '',
    label,
    onplay = null,
    onmenu = null,
  }: {
    source: string;
    /** The rows, each with its place in the whole list (one row open is known by it). */
    items: { r: PlaylistRow; i: number }[];
    /** What they are of: another, and nothing is open. */
    key?: string;
    label: string;
    onplay?: ((r: PlaylistRow) => void) | null;
    onmenu?: ((r: PlaylistRow) => void) | null;
  } = $props();

  let openRow = $state<number | null>(null);
  const snipOf = (r: PlaylistRow): SnippetTrack | null =>
    r.id && !r.missing ? { source, id: r.id, name: r.name ?? r.file, duration_secs: r.duration_secs ?? null } : null;
  const isOn = (t: SnippetTrack | null) => !!t && snippet.key === `${t.source}/${t.id}`;
  function snipTap(i: number, t: SnippetTrack) {
    if (openRow === i) {
      openRow = null;
      return endSnippet();
    }
    openRow = i;
    startSnippet(t);
  }
  $effect(() => {
    void key;
    untrack(() => {
      if (openRow !== null) endSnippet();
      openRow = null;
    });
  });
  onDestroy(() => openRow !== null && endSnippet());
</script>

<ol class="rows" aria-label={label}>
  {#each items as { r, i } (i)}
    {@const st = snipOf(r)}
    {@const off = !r.id || !!r.missing}
    <li>
      <Row
        size="large"
        title={r.name ?? r.file}
        sub={r.artist ?? ''}
        count={r.duration_secs ? clockText(r.duration_secs) : undefined}
        {off}
        label={onplay && !off ? `Play ${r.name ?? r.file}` : undefined}
        onclick={onplay && !off ? () => onplay(r) : undefined}
        attachMain={onmenu && !off ? fromAction(longpress, () => () => onmenu(r)) : undefined}
      >
        {#snippet lead()}<TrackCover src={r.id && !r.missing ? coverUrl({ source, id: r.id }) : null} />{/snippet}
        {#snippet action()}
          {#if st}
            {@const on = openRow === i && isOn(st) && snippet.sounding}
            <button class="snip" aria-label={openRow === i ? `Stop ${r.name ?? r.file}` : `Hear ${r.name ?? r.file}`} aria-pressed={openRow === i} onclick={() => snipTap(i, st)}>
              <SnippetMark open={openRow === i} share={on ? heardShare() : 0} />
            </button>
          {/if}
          {#if onmenu && !off}
            <button class="menu" aria-label={`Menu of ${r.name ?? r.file}`} onclick={() => onmenu(r)}><Icon name="moreVert" size={22} /></button>
          {/if}
        {/snippet}
      </Row>
      {#if openRow === i && st}
        <div class="snipped">
          <SnippetStrip track={st} />
        </div>
      {/if}
    </li>
  {/each}
</ol>

<style>
  .rows {
    margin: 0 -8px;
    padding: 0;
    list-style: none;
  }

  /* A row's snippet button (its mark, lib/SnippetMark) and its open row (A2's strip). */
  .snip,
  .menu {
    display: grid;
    place-items: center;
    padding: 4px;
    border: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
  }

  .menu {
    width: 40px;
    height: 40px;
    color: var(--muted);
  }

  .snipped {
    display: grid;
    padding: 0 16px 8px;
  }
</style>
