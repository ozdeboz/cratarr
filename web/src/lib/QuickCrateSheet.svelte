<script lang="ts">
  // A quick crate (0.11, lib/quickCrate): a few words typed, each read into a chip (its kind on it; a tap: the other
  // ways it reads, an artist's Sounds like or By, their own tracks, Remove). The crate shows at once, its count and
  // first tracks from the server's preview, and plays from there; where an artist is "like", the closeness curve
  // (lib/LikeCurve) loosens or tightens its cut. Save makes it one's own crate, the chips its conditions, and opens it;
  // Edit opens it in the crate builder. Closed unsaved, it is kept here among the last few (lib/quickCrates).
  import { untrack } from 'svelte';
  import { coverUrl, createCrate, likeCurve, previewCrate, search, type PlaylistRow } from './api';
  import { CRATE, loadCrates, startCrate } from './crates.svelte';
  import LikeCurve from './LikeCurve.svelte';
  import { curveKey, type CurveAsk } from './likeCurveAsk';
  import { hiddenLibraries } from './listFilter.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet, { type SheetAction } from './MenuSheet.svelte';
  import { viewPlaylist } from './panel.svelte';
  import { playCollection } from './play.svelte';
  import { prefs } from './prefs.svelte';
  import { CLOSE, TERM_KINDS, chipOf, conditionsOf, countAt, keepsOwn, minAt, nameOf, readAs, readingsFor, seedsOf, splitTerms, type Chip, type Reading } from './quickCrate';
  import { forgetQuick, keepQuick, quickCrates } from './quickCrates.svelte';
  import Row from './Row.svelte';
  import Seg from './Seg.svelte';
  import { toast } from './toast.svelte';
  import TrackCover from './TrackCover.svelte';
  import { withUndo } from './undo.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const from = quickCrates.from;
  let chips = $state<Chip[]>(from ? from.chips.map((c) => ({ ...c })) : []);
  let min = $state(from?.min ?? CLOSE);
  let text = $state('');
  /** Words being read (search asked); and those that read as nothing. */
  let reading = $state(0);
  let missed = $state<string[]>([]);
  /** The chip whose readings show (the sheet's second step). */
  let at = $state<number | null>(null);
  let saving = $state(false);
  let saved = false;

  const KIND: Record<Reading['kind'], string> = { artist: 'By', album: 'Album', label: 'Label', genre: 'Genre', style: 'Style', mood: 'Mood', tag: 'Tag', year: 'Year' };
  /** What a chip says it is: Like for an artist "like", a decade ("1970s") its own word. */
  const kindOf = (c: Chip) => (c.reading.kind === 'artist' && c.how === 'like' ? 'Like' : c.reading.kind === 'year' && c.reading.label.endsWith('s') ? 'Decade' : KIND[c.reading.kind]);
  const readingShown = (r: Reading) => `${r.kind === 'artist' ? 'Artist' : KIND[r.kind]} · ${r.label}`;

  /** Each word read: search's best of each kind, the best of them its chip (lib/quickCrate); one already a chip, once. */
  async function addTerms(words: string) {
    const terms = splitTerms(words).filter((t) => !chips.some((c) => c.term.toLowerCase() === t.toLowerCase()));
    if (!terms.length) return;
    reading++;
    try {
      const read = await Promise.all(
        terms.map(async (term) => {
          try {
            const r = await search(term, [...TERM_KINDS], 1);
            return chipOf(term, readingsFor(term, TERM_KINDS.flatMap((k) => r.groups[k]?.items.slice(0, 1) ?? [])));
          } catch {
            return chipOf(term, readingsFor(term, []));
          }
        }),
      );
      missed = terms.filter((_, i) => !read[i]);
      chips = [...chips, ...read.filter((c): c is Chip => !!c)];
    } finally {
      reading--;
    }
  }
  if (quickCrates.words.trim()) void addTerms(quickCrates.words);

  /** A comma (or a semicolon) ends a word: those before it read, the rest stays in the field; Enter reads it all. */
  function typed() {
    const cut = Math.max(text.lastIndexOf(','), text.lastIndexOf(';'));
    if (cut < 0) return;
    void addTerms(text.slice(0, cut));
    text = text.slice(cut + 1).trimStart();
  }
  function entered(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    void addTerms(text);
    text = '';
  }

  const conditions = $derived(conditionsOf(chips, min));
  const name = $derived(nameOf(chips));
  const seeds = $derived(seedsOf(chips));
  const own = $derived(keepsOwn(chips));

  // The closeness curve, for the artists "like" (and whether their own tracks are in it), counted among the tracks the
  // other chips leave (the owner: "Filtered curve in 0.11"), so its count is the preview's; asked as the preview is,
  // 300 ms after a change, and not again when only its cut moves.
  let curve = $state<number[] | null>(null);
  const curveAsk = $derived<CurveAsk>({ of: seeds, within: conditions, without: hiddenLibraries(), own });
  const curveAt = $derived(curveKey(curveAsk, prefs.source || undefined));
  $effect(() => {
    void curveAt;
    const ask = untrack(() => curveAsk);
    if (!ask.of.length) return void (curve = null);
    let gone = false;
    const t = setTimeout(() => {
      likeCurve(ask, prefs.source || undefined).then(
        (c) => !gone && (curve = c.alike),
        () => !gone && (curve = null),
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });

  // The crate as it is: its count and first tracks, asked 300 ms after a change (none without a chip: that would be
  // the whole library).
  let preview = $state<{ source: string; total: number; rows: PlaylistRow[] } | null>(null);
  let previewError = $state<string | null>(null);
  const key = $derived(JSON.stringify(conditions));
  $effect(() => {
    const k = key;
    if (!conditions.length) {
      preview = null;
      previewError = null;
      return;
    }
    let gone = false;
    const t = setTimeout(() => {
      previewCrate(JSON.parse(k), 30, prefs.source || undefined, { without: hiddenLibraries() }).then(
        (p) => {
          if (gone) return;
          preview = p;
          previewError = null;
        },
        (e) => !gone && (previewError = e instanceof Error ? e.message : 'Unexpected error'),
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });

  /** Played or shuffled as it is: the server's first 500 of it (a queue's most), here or where the output is. */
  async function play(how: 'play' | 'shuffle') {
    try {
      const p = await previewCrate(conditions, 500, prefs.source || undefined, { without: hiddenLibraries() });
      const ids = p.rows.flatMap((r) => (r.id && !r.missing ? [r.id] : []));
      if (ids.length) await playCollection({ kind: 'tracks', source: p.source, ids }, how, name);
    } catch (e) {
      toast.fail(e, "It couldn't be played");
    }
  }
  async function save() {
    saving = true;
    try {
      const made = await createCrate(name, conditions);
      saved = true;
      forgetQuick(chips);
      withUndo(`Created ${name}.`, [made.undo], { after: () => loadCrates() });
      await loadCrates();
      quickCrates.open = false;
      viewPlaylist(CRATE + made.id);
    } catch (e) {
      toast.fail(e, "The crate wasn't saved");
    } finally {
      saving = false;
    }
  }
  /** Into the crate builder, a new crate's draft: its conditions there to change one by one. */
  function edit() {
    saved = true;
    forgetQuick(chips);
    quickCrates.open = false;
    startCrate(name, conditions);
  }
  function close() {
    if (!saved && chips.length) keepQuick({ chips, min });
    onclose();
  }

  const none = $derived(!chips.length || preview?.total === 0);
  const actions = $derived<SheetAction[]>([
    { icon: 'play', label: 'Play', onclick: () => play('play'), disabled: none },
    { icon: 'shuffle', label: 'Shuffle', onclick: () => play('shuffle'), disabled: none },
    { icon: 'save', label: 'Save', onclick: save, disabled: none || saving, busy: saving },
    { icon: 'pencil', label: 'Edit', onclick: edit, disabled: !chips.length },
  ]);

  const chip = $derived(at !== null ? (chips[at] ?? null) : null);
  const setChip = (c: Chip) => at !== null && (chips[at] = c);
</script>

<MenuSheet
  label="Quick crate"
  icon="crates"
  title={chips.length ? name : 'Quick crate'}
  chips={preview && chips.length ? [`${preview.total.toLocaleString()} ${preview.total === 1 ? 'track' : 'tracks'}`] : []}
  actions={chip ? [] : actions}
  step={chip ? 'chip' : 'menu'}
  onback={chip ? () => (at = null) : null}
  onclose={close}
>
  {#if chip}
    <!-- A chip's step: the other ways its word reads; an artist's Sounds like or By, and their own tracks; Remove. -->
    {#if chip.reading.kind === 'artist' && chip.reading.id}
      <div class="how">
        <Seg
          label="Like or by"
          options={[
            { value: 'like', label: 'Sounds like' },
            { value: 'by', label: 'By' },
          ]}
          bind:value={() => chip.how ?? 'like', (v) => setChip({ ...chip, how: v as 'like' | 'by' })}
        />
      </div>
      {#if chip.how === 'like'}
        <MenuEntry icon="user" label="Their own tracks" switched={!!chip.own} onclick={() => setChip({ ...chip, own: !chip.own })} />
      {/if}
    {/if}
    {#each chip.others as r (r.kind + r.value)}
      <MenuEntry label={readingShown(r)} onclick={() => (setChip(readAs(chip, r)), (at = null))} />
    {/each}
    <MenuEntry
      icon="close"
      label="Remove"
      danger
      onclick={() => {
        chips = chips.filter((_, i) => i !== at);
        at = null;
      }}
    />
  {:else}
    <div class="quick">
      <input type="search" placeholder="Genre, artist, label, year…" aria-label="Words for a quick crate" bind:value={text} oninput={typed} onkeydown={entered} />
      {#if chips.length}
        <div class="chips">
          {#each chips as c, i (c.term)}
            <button class="chip" aria-pressed="true" onclick={() => (at = i)}><span class="n">{kindOf(c)}</span>{c.reading.label}</button>
          {/each}
        </div>
      {/if}
      {#if missed.length}<p class="note">Nothing found for {missed.join(', ')}</p>{/if}
      {#if seeds.length && curve?.length}
        {@const c = curve}
        <div class="near"><LikeCurve curve={c} count={countAt(c, min)} onchange={(n) => (min = minAt(c, n))} /></div>
      {/if}
      {#if previewError}
        <p class="error" role="alert">{previewError}</p>
      {:else if preview && chips.length}
        <ol class="preview" aria-label="The crate's first tracks">
          {#each preview.rows as r, i (i)}
            {@const src = preview.source}
            <li>
              <Row size="large" title={r.name ?? r.file} sub={r.artist ?? ''} off={!r.id || !!r.missing}>
                {#snippet lead()}<TrackCover src={r.id && !r.missing ? coverUrl({ source: src, id: r.id }) : null} />{/snippet}
              </Row>
            </li>
          {/each}
        </ol>
      {/if}
    </div>
    {#if !chips.length && !reading}
      {#each quickCrates.recent as k (nameOf(k.chips))}
        <MenuEntry
          icon="clock"
          label={nameOf(k.chips)}
          onclick={() => {
            chips = k.chips.map((c) => ({ ...c }));
            min = k.min;
          }}
        />
      {/each}
    {/if}
  {/if}
</MenuSheet>

<style>
  .quick {
    display: grid;
    gap: 12px;
    padding: 4px 16px 8px;
  }

  /* As a condition's value chips (lib/ConditionCard), all chosen. */
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: inherit;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .chip[aria-pressed='true'] {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 22%, var(--bg));
  }

  .n {
    color: var(--muted);
    font-size: 12px;
  }

  input[type='search'] {
    min-height: 36px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  .how {
    padding: 4px 16px 8px;
  }

  .preview {
    margin: 6px -16px 0;
    padding: 0;
    list-style: none;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }

  .error {
    margin: 0;
    color: var(--danger-ink);
  }
</style>
