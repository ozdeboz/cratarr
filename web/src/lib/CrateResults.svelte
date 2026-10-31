<script lang="ts">
  // A crate's tracks, as conditions select them (0.13, decision 0088; the owner, 2026-10-31: one start, one list): its
  // count with Shuffle and Play (and what the place adds, `actions`), the Records / Tracks switch a saved crate has
  // (lib/crateRecords), its tracks a page at a time, more as the list's end comes into sight, each with its round
  // snippet button opening it on its waveform (lib/SnippetStrip). A crate grabbed, made or changed (the panel's builder,
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
  import SnippetStrip from './SnippetStrip.svelte';
  import { endSnippet, snippet, startSnippet, type SnippetTrack } from './snippetPlayer.svelte';
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
  /** A record opened: the crate's tracks of it, back to the records. */
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

  // A row's snippet (0.12, the owner: "as buttons which when pressed expand a waveform as A2"): its round button plays
  // the track's liveliest 20 s and opens the row on its waveform, with Play from here; again, it stops and folds. One
  // row open at a time; leaving ends it.
  let openRow = $state<number | null>(null);
  const snipOf = (src: string, r: PlaylistRow): SnippetTrack | null =>
    r.id && !r.missing ? { source: src, id: r.id, name: r.name ?? r.file, duration_secs: r.duration_secs ?? null } : null;
  const isOn = (t: SnippetTrack | null) => !!t && snippet.key === `${t.source}/${t.id}`;
  function snipTap(i: number, t: SnippetTrack) {
    if (openRow === i) {
      openRow = null;
      return endSnippet();
    }
    openRow = i;
    startSnippet(t);
  }
  /** How far its snippet has sounded, 0..1 (its button's ring). */
  const heardShare = $derived(snippet.sounding && snippet.to > snippet.from ? Math.min(1, Math.max(0, (snippet.at - snippet.from) / (snippet.to - snippet.from))) : 0);
  // Other conditions: nothing open (more of the same rows leave it be).
  $effect(() => {
    void key;
    untrack(() => {
      if (openRow !== null) endSnippet();
      openRow = null;
    });
  });
  onDestroy(() => openRow !== null && endSnippet());
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
  {#if view === 'records' && openRecord}
    <SectionHead title={`${openRecord.title || 'Unknown album'} · ${holdsLine(openRecord.at.length, openRecord.of)}`} onback={() => (recordOpen = null)} />
    <ol class="preview" aria-label={`The crate's tracks of ${openRecord.title || 'an unknown album'}`}>
      {#each openRecord.at as i (i)}{@render trackRow(src, preview.rows[i], i)}{/each}
    </ol>
  {:else if view === 'records'}
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
    <ol class="preview" aria-label="The crate's tracks">
      {#each preview.rows as r, i (i)}{@render trackRow(src, r, i)}{/each}
    </ol>
  {/if}
  {#if preview.rows.length < preview.total}<div class="loading" bind:this={more}><MarkLoader size={40} /></div>{/if}
{:else if conditions.length}
  <!-- Its first answer on its way: the one loading rule's spinner (the owner, 2026-10-31: a grabbed crate showed
       nothing, no spinner either). A later change keeps the list shown until its answer. -->
  <Asking />
{/if}

<!-- A track of the crate: its cover, name, artist and length, its round snippet button opening it on its waveform. -->
{#snippet trackRow(src: string, r: PlaylistRow, i: number)}
  {@const st = snipOf(src, r)}
  <li>
    <Row size="large" title={r.name ?? r.file} sub={r.artist ?? ''} count={r.duration_secs ? clockText(r.duration_secs) : undefined} off={!r.id || !!r.missing}>
      {#snippet lead()}<TrackCover src={r.id && !r.missing ? coverUrl({ source: src, id: r.id }) : null} />{/snippet}
      {#snippet action()}
        {#if st}
          {@const on = openRow === i && isOn(st) && snippet.sounding}
          <button class="snip" aria-label={openRow === i ? `Stop ${r.name ?? r.file}` : `Hear ${r.name ?? r.file}`} aria-pressed={openRow === i} onclick={() => snipTap(i, st)}>
            <svg viewBox="0 0 36 36" width="36" height="36" aria-hidden="true">
              <circle class="ring" cx="18" cy="18" r="16" />
              {#if on}<circle class="fill" cx="18" cy="18" r="16" pathLength="100" stroke-dasharray="{heardShare * 100} 100" transform="rotate(-90 18 18)" />{/if}
              {#if openRow === i}<rect x="13" y="12" width="3.5" height="12" /><rect x="19.5" y="12" width="3.5" height="12" />{:else}<path d="M14 11v14l11-7z" />{/if}
            </svg>
          </button>
        {/if}
      {/snippet}
    </Row>
    {#if openRow === i && st}
      <div class="snipped">
        <SnippetStrip track={st} />
      </div>
    {/if}
  </li>
{/snippet}

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

  /* A row's snippet button (the owner's mockup C2) and its open row (A2's strip). */
  .snip {
    display: grid;
    place-items: center;
    padding: 4px;
    border: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
  }

  .snip svg {
    fill: currentColor;
  }

  .snip .ring {
    fill: none;
    stroke: var(--border);
    stroke-width: 3;
  }

  .snip .fill {
    fill: none;
    stroke: var(--art-accent, var(--accent));
    stroke-width: 3;
  }

  .snipped {
    display: grid;
    padding: 0 16px 8px;
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
