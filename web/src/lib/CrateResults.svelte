<script lang="ts">
  // A crate's tracks, as conditions select them (0.13, decision 0088; the owner, 2026-10-31: one start, one list): its
  // count with Shuffle and Play (and what the place adds, `actions`), the Records / Tracks switch a saved crate has
  // (lib/crateRecords), its tracks a page at a time, more as the list's end comes into sight, each with its round
  // snippet button opening it on its waveform (lib/SnippetRows); a record opens in a sheet (lib/RecordSheet). A crate grabbed, made or changed (the panel's builder,
  // lib/CrateEditor) shows this one list.
  import { onDestroy, untrack, type Snippet } from 'svelte';
  import { coverUrl, previewCrate, type Condition, type CrateName, type PlaylistRow } from './api';
  import { crateViewOf, holdsLine, orderRecords, RECORD_ORDERS, recordKey, recordsOf, type CrateView, type RecordOrder } from './crateRecords';
  import Asking from './Asking.svelte';
  import Icon from './Icon.svelte';
  import { plural } from './insights.svelte';
  import ListHead from './ListHead.svelte';
  import { hiddenLibraries } from './listFilter.svelte';
  import MarkLoader from './MarkLoader.svelte';
  import { playCollection } from './play.svelte';
  import { prefs } from './prefs.svelte';
  import { clockText } from './remote.svelte';
  import Row from './Row.svelte';
  import SectionHead from './SectionHead.svelte';
  import Seg from './Seg.svelte';
  import { shuffled } from './shuffle';
  import RecordSheet from './RecordSheet.svelte';
  import SnippetRows from './SnippetRows.svelte';
  import { matcherModule } from './matcherLoad';
  import { spreadRecords } from './spread';
  import { toast } from './toast.svelte';
  import TrackCover from './TrackCover.svelte';

  let {
    conditions,
    name,
    total = $bindable(null),
    views = 'quick',
    actions,
    onnames,
  }: {
    conditions: readonly Condition[];
    /** Its name, for what plays. */
    name: string;
    /** How many it selects (the panel shows it under the name). */
    total?: number | null;
    /** Whose Records / Tracks choice it keeps (a crate's id; a crate being grabbed or made: "quick"). */
    views?: string;
    /** What the place adds before Shuffle and Play (Save, its menu). */
    actions?: Snippet;
    /** The server's names for the ids the conditions hold (decision 0088), as each answer brings them. */
    onnames?: (names: Record<string, CrateName>) => void;
  } = $props();

  // Its count and its tracks a page at a time, asked 300 ms after a change (none without a condition: that would be the
  // whole library); more as the list's end comes into sight.
  const PAGE = 50;
  let preview = $state<{ source: string; total: number; rows: PlaylistRow[] } | null>(null);
  let previewError = $state<string | null>(null);
  const key = $derived(JSON.stringify(conditions));
  $effect(() => {
    const k = key;
    if (!untrack(() => conditions.length)) {
      preview = null;
      previewError = null;
      total = null;
      return;
    }
    let gone = false;
    const t = setTimeout(() => {
      previewCrate(JSON.parse(k), PAGE, prefs.source || undefined, { without: hiddenLibraries() }).then(
        (p) => {
          if (gone) return;
          preview = p;
          previewError = null;
          total = p.total;
          if (p.names) onnames?.(p.names);
        },
        (e) => {
          if (gone) return;
          previewError = e instanceof Error ? e.message : 'Unexpected error';
          total = null;
        },
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });

  let more = $state<HTMLElement>();
  let loadingMore = false;
  $effect(() => {
    const el = more;
    if (!el) return;
    const seen = new IntersectionObserver(
      async ([e]) => {
        const p = preview;
        if (!e.isIntersecting || loadingMore || !p || p.rows.length >= p.total) return;
        const k = key;
        loadingMore = true;
        try {
          const next = await previewCrate(JSON.parse(k), PAGE, prefs.source || undefined, { without: hiddenLibraries() }, p.rows.length);
          if (key === k && preview === p) preview = { ...p, total: next.total, rows: [...p.rows, ...next.rows] };
        } catch (err) {
          if (key === k) previewError = err instanceof Error ? err.message : 'Unexpected error';
        } finally {
          loadingMore = false;
        }
      },
      { rootMargin: '400px' },
    );
    seen.observe(el);
    return () => seen.disconnect();
  });

  // Records or tracks, as a saved crate (lib/crateRecords): kept for this device as the crates' own views are.
  const VIEWS: { value: CrateView; label: string }[] = [
    { value: 'records', label: 'Records' },
    { value: 'tracks', label: 'Tracks' },
  ];
  const view = $derived(crateViewOf(views, prefs.crateViews));
  const setView = (v: CrateView) => (prefs.crateViews = { ...prefs.crateViews, [views]: v });
  const records = $derived(
    view === 'records' && preview ? orderRecords(recordsOf(preview.rows.map((r, i) => ({ r, i }))), prefs.crateRecordSort.key, prefs.crateRecordSort.down) : [],
  );
  /** A record opened: the crate's tracks of it, in a sheet over the list (lib/RecordSheet; the owner, 2026-11-02). */
  let recordOpen = $state<string | null>(null);
  const openRecord = $derived(view === 'records' && recordOpen ? (records.find((r) => r.key === recordOpen) ?? null) : null);
  $effect(() => {
    void key;
    untrack(() => (recordOpen = null));
  });

  const none = $derived(!conditions.length || preview?.total === 0);
  let starting = $state<'play' | 'shuffle' | null>(null);
  /** Played or shuffled: the server's first 500 of it (a queue's most), here or where the output is; in the crate's
   *  order or a fair shuffle's draw, a record's tracks kept apart either way (lib/spread, the rules crate's from matcher.wasm; the owner, 2026-10-31: "too
   *  much repetition of records in the queue"). */
  async function play(how: 'play' | 'shuffle') {
    starting = how;
    try {
      const p = await previewCrate([...conditions], 500, prefs.source || undefined, { without: hiddenLibraries() });
      const playable = p.rows.filter((r) => r.id && !r.missing);
      const ids = spreadRecords(await matcherModule(), how === 'shuffle' ? shuffled(playable) : playable, (r) => recordKey(r)).map((r) => r.id!);
      if (ids.length) await playCollection({ kind: 'tracks', source: p.source, ids }, 'play', name);
    } catch (e) {
      toast.fail(e, "It couldn't be played");
    } finally {
      starting = null;
    }
  }

</script>

<div class="sum">
  <b>{preview ? `${preview.total.toLocaleString()} ${preview.total === 1 ? 'track' : 'tracks'}` : ''}</b>
  {@render actions?.()}
  <button class="icon" aria-label="Shuffle" title="Shuffle" disabled={none || !!starting} onclick={() => play('shuffle')}><Icon name="shuffle" size={22} /></button>
  <button class="icon play" aria-label="Play" title="Play" disabled={none || !!starting} onclick={() => play('play')}><Icon name="play" size={22} /></button>
</div>
{#if previewError}
  <p class="error" role="alert">{previewError}</p>
{:else if preview}
  {@const src = preview.source}
  <div class="views"><Seg label="Show the crate as" options={VIEWS} bind:value={() => view, setView} /></div>
  {#if view === 'records'}
    <ListHead
      fields={RECORD_ORDERS}
      bind:value={() => prefs.crateRecordSort.key, (v) => (prefs.crateRecordSort = { key: v as RecordOrder, down: prefs.crateRecordSort.down })}
      bind:down={() => prefs.crateRecordSort.down, (d) => (prefs.crateRecordSort = { key: prefs.crateRecordSort.key, down: d })}
      count={plural(records.length, 'record')}
    />
    <ol class="preview" aria-label="The crate's records">
      {#each records as rec (rec.key)}
        {@const first = preview.rows[rec.at[0]]}
        <li>
          <Row size="large" title={rec.title || 'Unknown album'} sub={[rec.artist, holdsLine(rec.at.length, rec.of)].filter(Boolean).join(' · ')} label={`Open ${rec.title || 'an unknown album'}`} onclick={() => (recordOpen = rec.key)}>
            {#snippet lead()}<TrackCover src={first?.id && !first.missing ? coverUrl({ source: src, id: first.id }) : null} />{/snippet}
          </Row>
        </li>
      {/each}
    </ol>
  {:else}
    <SnippetRows source={src} items={preview.rows.map((r, i) => ({ r, i }))} {key} label="The crate's tracks" />
  {/if}
  {#if openRecord}
    {@const rec = openRecord}
    <RecordSheet
      source={src}
      title={rec.title || 'Unknown album'}
      chips={[rec.artist ?? '', holdsLine(rec.at.length, rec.of)].filter(Boolean)}
      items={rec.at.map((i) => ({ r: preview!.rows[i], i }))}
      onclose={() => (recordOpen = null)}
    />
  {/if}
  {#if preview.rows.length < preview.total}<div class="loading" bind:this={more}><MarkLoader size={40} /></div>{/if}
{:else if conditions.length}
  <!-- Its first answer on its way: the one loading rule's spinner (the owner, 2026-10-31: a grabbed crate showed
       nothing, no spinner either). A later change keeps the list shown until its answer. -->
  <Asking />
{/if}

<style>
  .sum {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .sum b {
    flex: 1;
    font-size: 16px;
    font-variant-numeric: tabular-nums;
  }

  /* As a crate's action bar (lib/PlaylistView): round buttons, Play the accent one; the place's own (`actions`) too. */
  .sum :global(.icon) {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .sum :global(.icon.play) {
    background: var(--accent);
    color: var(--accent-text);
  }

  .sum :global(.icon:disabled) {
    opacity: 0.4;
    cursor: default;
  }

  /* Records or tracks, as a saved crate's (lib/crateRecords). */
  .views {
    display: flex;
  }

  .loading {
    display: grid;
    place-items: center;
    padding: 16px 0;
  }







  .preview {
    margin: 0 -8px;
    padding: 0;
    list-style: none;
  }

  .error {
    margin: 0;
    color: var(--danger-ink);
  }
</style>
