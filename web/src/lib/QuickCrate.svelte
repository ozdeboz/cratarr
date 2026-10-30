<script lang="ts">
  // A quick crate at the top of Crates (0.12; the owner, 2026-10-30: the crate search's mockup B, "B with A's graph"):
  // the crate read as a sentence (lib/quickCrate sentenceOf), its terms underlined, the words between muted, ending in
  // the field where more is typed ("and…"; the whole line read, a comma or Enter). A term tapped reads another way
  // (lib/QuickTermSheet). Where an artist is "like", the closeness curve (lib/LikeCurve) under it; then its count with
  // Save, Shuffle and Play, its menu (Edit in the crate builder, Start over), and its first tracks. Nothing typed: the
  // last few not saved, to come back to. Its state is lib/quickCrates': it stays while the page is left.
  import { onDestroy, untrack } from 'svelte';
  import { coverUrl, createCrate, likeCurve, previewCrate, search, type PlaylistRow } from './api';
  import { CRATE, loadCrates, startCrate } from './crates.svelte';
  import Icon from './Icon.svelte';
  import LikeCurve from './LikeCurve.svelte';
  import { curveKey, type CurveAsk } from './likeCurveAsk';
  import { hiddenLibraries } from './listFilter.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { viewPlaylist } from './panel.svelte';
  import { playCollection } from './play.svelte';
  import { prefs } from './prefs.svelte';
  import { CLOSE, TERM_KINDS, chipOf, conditionsOf, countAt, keepsOwn, minAt, nameOf, readLine, seedsOf, sentenceOf, type Chip, type Found } from './quickCrate';
  import { forgetQuick, keepQuick, quickCrates } from './quickCrates.svelte';
  import QuickTermSheet from './QuickTermSheet.svelte';
  import { clockText } from './remote.svelte';
  import Row from './Row.svelte';
  import { toast } from './toast.svelte';
  import TrackCover from './TrackCover.svelte';
  import { withUndo } from './undo.svelte';

  const chips = $derived(quickCrates.chips);
  const min = $derived(quickCrates.min);
  let text = $state('');
  /** Words that read as nothing. */
  let missed = $state<string[]>([]);
  /** The term whose sheet shows. */
  let at = $state<number | null>(null);
  let menuOpen = $state(false);
  let saving = $state(false);

  /** How many of each kind a phrase's readings offer (the owner, 2026-10-30: the right one could not be chosen with one). */
  const EACH = 3;
  const ask = (phrase: string): Promise<Found> =>
    search(phrase, [...TERM_KINDS], EACH).then((r) => TERM_KINDS.flatMap((k) => r.groups[k]?.items.slice(0, EACH) ?? []));

  /** The words typed, read as a whole line (lib/quickCrate readLine), each phrase a term of its best reading; one
   *  already in it, once. */
  async function addTerms(words: string) {
    if (!words.trim()) return;
    const read = (await readLine(words, ask)).filter((r) => !quickCrates.chips.some((c) => c.term.toLowerCase() === r.term.toLowerCase()));
    missed = read.filter((r) => !r.readings.length).map((r) => r.term);
    quickCrates.chips = [...quickCrates.chips, ...read.flatMap((r) => chipOf(r.term, r.readings) ?? [])];
  }
  // A search's words (lib/quickCrates openQuickCrate), read as they come.
  $effect(() => {
    const words = quickCrates.words;
    if (!words) return;
    untrack(() => {
      quickCrates.words = '';
      missed = [];
      void addTerms(words);
    });
  });

  /** A comma (or a semicolon) ends what is before it: read, the rest stays in the field; Enter reads it all. */
  function typed() {
    const cut = Math.max(text.lastIndexOf(','), text.lastIndexOf(';'));
    if (cut < 0) return;
    void addTerms(text.slice(0, cut));
    text = text.slice(cut + 1).trimStart();
  }
  function entered(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void addTerms(text);
      text = '';
    } else if (e.key === 'Backspace' && !text && quickCrates.chips.length) {
      // An empty field's Backspace opens the last term (its Remove a tap away), as a token field's would take it.
      at = quickCrates.chips.length - 1;
    }
  }

  const parts = $derived(sentenceOf(chips));
  const conditions = $derived(conditionsOf(chips, min));
  const name = $derived(nameOf(chips));
  const seeds = $derived(seedsOf(chips));
  const own = $derived(keepsOwn(chips));
  const likedNames = $derived(chips.filter((c) => c.reading.kind === 'artist' && c.how === 'like' && c.reading.id).map((c) => c.reading.label));

  // The closeness curve, for the artists "like", counted among the tracks the other terms leave (the owner: "Filtered
  // curve in 0.11"), so its count is the preview's; asked 300 ms after a change, not again when only its cut moves.
  let curve = $state<number[] | null>(null);
  const curveAsk = $derived<CurveAsk>({ of: seeds, within: conditions, without: hiddenLibraries(), own });
  const curveAt = $derived(curveKey(curveAsk, prefs.source || undefined));
  $effect(() => {
    void curveAt;
    const asked = untrack(() => curveAsk);
    if (!asked.of.length) return void (curve = null);
    let gone = false;
    const t = setTimeout(() => {
      likeCurve(asked, prefs.source || undefined).then(
        (c) => !gone && (curve = c.alike),
        () => !gone && (curve = null),
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });

  // The crate as it is: its count and first tracks, asked 300 ms after a change (none without a term: that would be the
  // whole library).
  let preview = $state<{ source: string; total: number; rows: PlaylistRow[] } | null>(null);
  let previewError = $state<string | null>(null);
  const key = $derived(JSON.stringify(conditions));
  $effect(() => {
    const k = key;
    if (!untrack(() => conditions.length)) {
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

  const none = $derived(!chips.length || preview?.total === 0);
  let starting = $state<'play' | 'shuffle' | null>(null);
  /** Played or shuffled as it is: the server's first 500 of it (a queue's most), here or where the output is. */
  async function play(how: 'play' | 'shuffle') {
    starting = how;
    try {
      const p = await previewCrate(conditions, 500, prefs.source || undefined, { without: hiddenLibraries() });
      const ids = p.rows.flatMap((r) => (r.id && !r.missing ? [r.id] : []));
      if (ids.length) await playCollection({ kind: 'tracks', source: p.source, ids }, how, name);
    } catch (e) {
      toast.fail(e, "It couldn't be played");
    } finally {
      starting = null;
    }
  }
  /** Gone from the top (saved, edited, started over): nothing being made. */
  function clear() {
    quickCrates.chips = [];
    quickCrates.min = CLOSE;
    text = '';
    missed = [];
  }
  async function save() {
    saving = true;
    try {
      const made = await createCrate(name, conditions);
      forgetQuick(chips);
      clear();
      withUndo(`Created ${name}.`, [made.undo], { after: () => loadCrates() });
      await loadCrates();
      viewPlaylist(CRATE + made.id);
    } catch (e) {
      toast.fail(e, "The crate wasn't saved");
    } finally {
      saving = false;
    }
  }
  /** Into the crate builder, a new crate's draft: its conditions there to change one by one. */
  function edit() {
    menuOpen = false;
    const [n, c] = [name, conditions];
    forgetQuick(chips);
    clear();
    startCrate(n, c);
  }
  function startOver() {
    menuOpen = false;
    keepQuick({ chips, min });
    clear();
  }
  // Left unsaved (the page left, the browser closed): kept among the last few, and still here on coming back.
  const keep = () => quickCrates.chips.length && keepQuick({ chips: quickCrates.chips, min: quickCrates.min });
  $effect(() => {
    addEventListener('pagehide', keep);
    return () => removeEventListener('pagehide', keep);
  });
  onDestroy(keep);
</script>

<section class="quick" aria-label="Make a crate">
  <p class="sentence" class:empty={!chips.length}>
    {#each parts as p, k (k)}{#if k && !p.tight}{' '}{/if}{#if p.chip !== undefined}{@const c = chips[p.chip]}<button
          class="term"
          class:like={c?.reading.kind === 'artist' && c.how === 'like'}
          onclick={() => (at = p.chip ?? null)}>{p.text}</button
        >{:else}<span class="w">{p.text}</span>{/if}{/each}{#if chips.length}{' '}{/if}<input
      type="search"
      placeholder={chips.length ? 'and…' : 'hip hop 90s Miles Davis'}
      aria-label={chips.length ? 'More words for the crate' : 'Words for a crate'}
      bind:value={text}
      oninput={typed}
      onkeydown={entered}
    />
  </p>
  {#if missed.length}<p class="note">Nothing found for {missed.join(', ')}</p>{/if}

  {#if chips.length}
    {#if seeds.length && curve?.length}
      {@const c = curve}
      <div class="near">
        <p class="alike"><span>Alike {likedNames.join(' and ')}</span><b>at least {Math.round(min)}%</b></p>
        <LikeCurve curve={c} count={countAt(c, min)} onchange={(n) => (quickCrates.min = minAt(c, n))} />
      </div>
    {/if}
    <div class="sum">
      <b>{preview ? `${preview.total.toLocaleString()} ${preview.total === 1 ? 'track' : 'tracks'}` : ''}</b>
      <button class="icon" aria-label="Save as a crate" title="Save" disabled={none || saving} onclick={save}><Icon name="save" size={22} /></button>
      <button class="icon" aria-label="Shuffle" title="Shuffle" disabled={none || !!starting} onclick={() => play('shuffle')}><Icon name="shuffle" size={22} /></button>
      <button class="icon play" aria-label="Play" title="Play" disabled={none || !!starting} onclick={() => play('play')}><Icon name="play" size={22} /></button>
      <button class="icon" aria-label="The crate's menu" onclick={() => (menuOpen = true)}><Icon name="moreVert" size={22} /></button>
    </div>
    {#if previewError}
      <p class="error" role="alert">{previewError}</p>
    {:else if preview}
      <ol class="preview" aria-label="The crate's first tracks">
        {#each preview.rows as r, i (i)}
          {@const src = preview.source}
          <li>
            <Row size="large" title={r.name ?? r.file} sub={r.artist ?? ''} count={r.duration_secs ? clockText(r.duration_secs) : undefined} off={!r.id || !!r.missing}>
              {#snippet lead()}<TrackCover src={r.id && !r.missing ? coverUrl({ source: src, id: r.id }) : null} />{/snippet}
            </Row>
          </li>
        {/each}
      </ol>
    {/if}
  {:else if quickCrates.recent.length}
    <ul class="recent" aria-label="Made here, not saved">
      {#each quickCrates.recent as k (nameOf(k.chips))}
        <li>
          <Row
            size="medium"
            title={nameOf(k.chips)}
            onclick={() => {
              quickCrates.chips = k.chips.map((c) => ({ ...c }));
              quickCrates.min = k.min;
            }}
          >
            {#snippet lead()}<span class="lead"><Icon name="clock" size={20} /></span>{/snippet}
          </Row>
        </li>
      {/each}
    </ul>
  {/if}
</section>

{#if at !== null && chips[at]}
  {@const i = at}
  <QuickTermSheet
    chip={chips[i]}
    {ask}
    onchange={(c: Chip) => (quickCrates.chips = quickCrates.chips.map((x, k) => (k === i ? c : x)))}
    onremove={() => (quickCrates.chips = quickCrates.chips.filter((_, k) => k !== i))}
    onclose={() => (at = null)}
  />
{/if}

{#if menuOpen}
  <MenuSheet label="The crate being made" icon="crates" title={name} onclose={() => (menuOpen = false)}>
    <MenuEntry icon="pencil" label="Edit" onclick={edit} />
    <MenuEntry icon="close" label="Start over" onclick={startOver} />
  </MenuSheet>
{/if}

<style>
  .quick {
    display: grid;
    gap: 10px;
    margin: 4px 0 14px;
  }

  /* The crate as a sentence (mockup B): its terms in the text's own weight, underlined, the words between muted. */
  .sentence {
    margin: 0;
    padding: 4px 4px 0;
    font-size: 22px;
    font-weight: 600;
    line-height: 1.55;
    text-wrap: pretty;
  }

  .w {
    color: var(--muted);
    font-weight: 400;
  }

  .term {
    padding: 0 0 1px;
    border: 0;
    border-bottom: 2px solid var(--border);
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .term.like {
    border-bottom-color: var(--accent);
  }

  .term:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: 3px;
  }

  /* Where more is typed: the sentence's end, as wide as what is in it. */
  input[type='search'] {
    field-sizing: content;
    min-width: 5ch;
    max-width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 18px;
    font-weight: 400;
    caret-color: var(--accent);
  }

  input[type='search']:focus {
    outline: none;
  }

  /* Nothing typed yet: a field of its own, the list's width. */
  .empty {
    padding: 0;
  }

  .empty input[type='search'] {
    box-sizing: border-box;
    width: 100%;
    field-sizing: fixed;
    min-height: 40px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    font-size: 15px;
  }

  .near {
    display: grid;
    gap: 4px;
  }

  .alike {
    display: flex;
    justify-content: space-between;
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }

  .alike b {
    color: var(--text);
    font-weight: 600;
  }

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

  /* As a crate's action bar (lib/PlaylistView): round buttons, Play the accent one. */
  .icon {
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

  .icon.play {
    background: var(--accent);
    color: var(--accent-text);
  }

  .icon:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .preview,
  .recent {
    margin: 0 -8px;
    padding: 0;
    list-style: none;
  }

  .lead {
    display: grid;
    place-items: center;
    width: 40px;
    color: var(--muted);
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
