<script lang="ts">
  // Suggested tracks, at the foot of one's own playlist (the owner, 2026-10-23: "potential tracks to add to a playlist";
  // for 76): a few of what sounds most like it, its own left out, leaning towards tracks in no playlist yet. The
  // playlist's hidden crate (`like:list:<id>`, the server's) holds them: + adds one to the playlist, ✕ turns it down for
  // good (the crate's left-out list), both with Undo, and the next takes its place; Refresh shows the next few; the
  // heading's › opens all of them as the crate. A tap on a row plays the suggestions from it.
  import { untrack } from 'svelte';
  import { appendTracks, coverUrl, crateRows, leaveOutOfCrate, type PlaylistRow } from './api';
  import AvoidLine from './AvoidLine.svelte';
  import Cover from './Cover.svelte';
  import { LIKE, viewLike } from './crates.svelte';
  import Icon from './Icon.svelte';
  import { playCollection } from './play.svelte';
  import SectionHead from './SectionHead.svelte';
  import { toast } from './toast.svelte';
  import { undos, withUndo } from './undo.svelte';

  let { id, name, count }: { id: string; name: string; count: number } = $props();

  /** How many show at a time. */
  const SHOW = 10;
  const crate = $derived(`${LIKE}list:${id}`);

  let rows = $state<PlaylistRow[]>([]);
  let source = $state('');
  let total = $state(0);
  let offset = $state(0);
  let busy = $state(false);
  /** Bumped when something is turned down or comes back: what it learned is read again. */
  let turned = $state(0);

  async function load(at: number) {
    const [c, n] = [crate, count];
    try {
      const r = await crateRows(c, at, SHOW);
      if (c !== crate || n !== count) return;
      rows = r.rows.filter((x) => x.id && !x.missing);
      source = r.source;
      total = r.total;
    } catch {
      rows = [];
    }
  }
  // Read again when the playlist changes (a track added here or anywhere), and after an undo or a redo; another
  // playlist's from the top.
  let shownFor = '';
  $effect(() => {
    void [count, undos.n, turned];
    if (crate !== shownFor) {
      shownFor = crate;
      offset = 0;
    }
    void load(untrack(() => offset));
  });

  function next() {
    offset = offset + SHOW < total ? offset + SHOW : 0;
    void load(offset);
  }

  async function add(r: PlaylistRow) {
    busy = true;
    try {
      const done = await appendTracks(id, source, [r.id!]);
      withUndo(`Added to ${name}.`, [done.undo]);
    } catch (e) {
      toast.fail(e, "The track wasn't added");
    } finally {
      busy = false;
    }
  }

  async function turnDown(r: PlaylistRow) {
    busy = true;
    try {
      const done = await leaveOutOfCrate(crate, source, { ids: [r.id!] });
      withUndo(`Not suggested for ${name} again.`, [done.undo]);
      turned++;
    } catch (e) {
      toast.fail(e, "The suggestion wasn't turned down");
    } finally {
      busy = false;
    }
  }

  async function play(r: PlaylistRow) {
    try {
      await playCollection({ kind: 'tracks', source, ids: rows.map((x) => x.id!) }, 'play', `Sounds like ${name}`, r.id!);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
</script>

{#if rows.length}
  <section aria-label="Suggested tracks">
    <SectionHead title="Suggested tracks" onmore={() => viewLike(`list:${id}`)} />
    <AvoidLine {crate} {name} round={turned + undos.n} onchange={() => turned++} />
    <ul>
      {#each rows as r (r.entry)}
        <li>
          <button class="track" onclick={() => play(r)}>
            <span class="cover"><Cover src={coverUrl({ source, id: r.id!, album_id: r.album_id })} /></span>
            <span class="text">
              <span class="title">{r.name ?? r.file}</span>
              <span class="sub">{[r.artist, r.album].filter(Boolean).join(' · ')}</span>
            </span>
          </button>
          <button class="act" disabled={busy} onclick={() => turnDown(r)} aria-label="Not for {name}"><Icon name="close" size={20} /></button>
          <button class="act add" disabled={busy} onclick={() => add(r)} aria-label="Add to {name}"><Icon name="plus" size={22} /></button>
        </li>
      {/each}
    </ul>
    {#if total > SHOW}
      <div class="more"><button onclick={next}><Icon name="reset" size={18} />Refresh</button></div>
    {/if}
  </section>
{/if}

<style>
  section {
    margin-top: 24px;
  }

  ul {
    margin: 0;
    padding: 4px 8px 0 16px;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .track {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 12px;
    min-width: 0;
    padding: 6px 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .cover {
    flex: none;
    width: 44px;
    height: 44px;
    overflow: hidden;
    border-radius: 4px;
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .title,
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub {
    color: var(--muted);
    font-size: 0.85rem;
  }

  .act {
    display: grid;
    flex: none;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .act.add {
    color: var(--accent-ink);
  }

  .act:disabled {
    opacity: 0.5;
  }

  .more {
    padding: 8px 16px 0;
  }

  .more button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
</style>
