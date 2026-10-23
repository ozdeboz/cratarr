<script lang="ts">
  // A playlist's missing tracks, Health's third level (the owner, 2026-10-23: "a search function"): each line whose file
  // is gone, found again by what the line still says (the same title at heart by the same artist, wherever it lies now;
  // the owner, 2026-10-24: the words were too strict), the one answer ringed; the words prefilled can be changed and
  // searched. A found track, tapped, is what the line points at from then on (the Repair tab's own fix, one line).
  import { untrack } from 'svelte';
  import { albumCoverUrl, findTracks, playlistRows, relinkLine, type FoundTrack } from './api';
  import { withUndo } from './undo.svelte';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import { toast } from './toast.svelte';

  let { playlist }: { playlist: string } = $props();

  /** `typed`: the words were changed by hand (searched as words; until then the line is found again by what it says). */
  type Missing = { entry: string; query: string; typed: boolean; found: FoundTrack[] | null; busy: boolean; fixed: string | null };
  let lines = $state<Missing[] | null>(null);
  /** What a line still says, as words to search by: its file's name without number or extension, and its folder's. */
  function words(entry: string): string {
    const parts = entry.split('/');
    const file = (parts.pop() ?? '').replace(/\.[a-z0-9]+$/i, '').replace(/^[\d\s.\-_]+/, '');
    const folder = (parts.pop() ?? '').replace(/\(\d{4}\)/, '').split(' - ')[0];
    return [folder, file].filter(Boolean).join(' ').replace(/[_]+/g, ' ').trim();
  }
  async function search(m: Missing) {
    if (!m.query.trim()) return;
    m.busy = true;
    try {
      m.found = await findTracks(m.query, m.typed ? undefined : m.entry);
    } catch (e) {
      toast.fail(e, "The library couldn't be searched");
    } finally {
      m.busy = false;
    }
  }
  $effect(() => {
    const id = playlist;
    let gone = false;
    lines = null;
    playlistRows(id).then(
      (r) => {
        if (gone) return;
        lines = r.rows.filter((x) => x.missing).map((x) => ({ entry: x.entry, query: words(x.entry), typed: false, found: null, busy: false, fixed: null }));
        // Each found again at once, by what its line says.
        untrack(() => lines!.forEach(search));
      },
      () => !gone && (lines = []),
    );
    return () => (gone = true);
  });
  async function fix(m: Missing, t: FoundTrack) {
    try {
      const { undo } = await relinkLine({ playlist, from: m.entry, to: t.line });
      withUndo(`${t.title} is back in ${playlist}.`, [undo]);
      m.fixed = `${t.title} · ${t.artist}`;
      m.found = null;
    } catch (e) {
      toast.fail(e, "The line couldn't be changed");
    }
  }
</script>

{#if lines === null}
  <p class="muted">…</p>
{:else if !lines.length}
  <p class="muted">Nothing missing.</p>
{:else}
  <ul class="lines">
    {#each lines as m (m.entry)}
      <li>
        <p class="entry" class:fixed={m.fixed}>{m.entry.split('/').pop()}<small>{m.entry.split('/').slice(0, -1).join('/')}</small></p>
        {#if m.fixed}
          <p class="now"><Icon name="check" size={16} />{m.fixed}</p>
        {:else}
          <form class="search" onsubmit={(e) => (e.preventDefault(), search(m))}>
            <Icon name="search" size={18} />
            <input bind:value={m.query} oninput={() => (m.typed = true)} aria-label="Search the library" />
            <button type="submit" disabled={m.busy}>Search</button>
          </form>
          {#if m.found}
            {#if !m.found.length}
              <p class="muted">Nothing found.</p>
            {:else}
              <ul class="found">
                {#each m.found as t (t.id)}
                  <li class:sure={t.sure}>
                    <button onclick={() => fix(m, t)}>
                      <span class="cover"><Cover src={albumCoverUrl('cratarr', t.album_id)} /></span>
                      <span class="words"><b>{t.title}</b><small>{t.artist} · {t.album}</small></span>
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          {/if}
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .muted {
    margin: 0;
    color: var(--muted);
  }

  .lines {
    display: grid;
    gap: 18px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .entry {
    display: grid;
    gap: 2px;
    margin: 0 0 8px;
    font-weight: 500;
    overflow-wrap: anywhere;
  }

  .entry.fixed {
    color: var(--muted);
    text-decoration: line-through;
  }

  .entry small {
    color: var(--muted);
    font-size: 0.82rem;
    font-weight: 400;
  }

  .now {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--accent-ink);
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 4px 4px 12px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--muted);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    outline: none;
  }

  .search button {
    padding: 6px 14px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .found {
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
  }

  .found button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 4px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .found button:hover {
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  /* The one answer (the same title at heart by the same artist): ringed, a tap away. */
  .found li.sure button {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .cover {
    flex: none;
    width: 40px;
    height: 40px;
    overflow: hidden;
    border-radius: 6px;
  }

  .words {
    display: grid;
    min-width: 0;
  }

  .words b,
  .words small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .words b {
    font-weight: 500;
  }

  .words small {
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>
