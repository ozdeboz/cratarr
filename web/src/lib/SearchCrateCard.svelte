<script module lang="ts">
  import type { CrateCover } from './api';
  import type { Chip, Modes } from './quickCrate';

  type Shown = { chips: Chip[]; modes: Modes; preview: { source: string; total: number; covers?: CrateCover[] } };
  /** The card last shown and the words it was for: a card made anew for the same words (a page's search handed to
   *  global search's, each its own card) starts as it was, not gone until its tracks are asked again (the owner,
   *  2026-10-31: it hid for a moment, "I see no use for this at all"). */
  let last: { q: string; shown: Shown } | null = null;
  /** Cards showing a crate now: a page's own search then found something, and its "No … for" says nothing (the owner,
   *  2026-11-02: "No crates for Hip hop 90s" over the crate those words made; lib/ContentControls). */
  export const crateCards = $state({ shown: 0 });
</script>

<script lang="ts">
  // A search's words read as a crate (0.12; the owner, 2026-10-30: "a crate card first"): where they read as two terms
  // or more of two kinds or more (lib/quickCrate crateOfLine; read once, lib/globalSearch `crate`), the first result is
  // the crate they make. One card for global search's results and every page's own (0.13; the owner, 2026-10-31), shown
  // only once its count is in and it holds a track (crateShows): nothing for one word, one kind or an empty crate,
  // nothing while it loads. Its look is Home's week card (the owner, 2026-10-31: "A · Week", lib/CoverCard): a strip of
  // its covers, its sentence as the heading, its terms as the week's pills, each opening its menu (Leave out there too),
  // "+ word" for another; Play, Grab this crate (the panel's builder, lib/quickCrates grabCrate: the one place a crate is
  // shown and built) and its count.
  import { untrack } from 'svelte';
  import { coverUrl, previewCrate, search } from './api';
  import CoverCard from './CoverCard.svelte';
  import { globalSearch } from './globalSearch.svelte';
  import Icon from './Icon.svelte';
  import { plural } from './insights.svelte';
  import { hiddenLibraries } from './listFilter.svelte';
  import { playCollection } from './play.svelte';
  import { prefs } from './prefs.svelte';
  import QuickTermSheet from './QuickTermSheet.svelte';
  import { TERM_KINDS, conditionsOf, crateShows, nameOf, readLine, readOnto, seedsOf, sentenceOf, type Found } from './quickCrate';
  import { grabCrate } from './quickCrates.svelte';
  import { words } from './search';
  import { toast } from './toast.svelte';

  let { q }: { q: string } = $props();

  /** How many of each kind a phrase's readings offer, as the crate search asks. */
  const EACH = 3;
  const ask = (phrase: string): Promise<Found> =>
    search(phrase, [...TERM_KINDS], EACH, 0, hiddenLibraries()).then((r) => TERM_KINDS.flatMap((k) => r.groups[k]?.items.slice(0, EACH) ?? []));
  /** Its count and its first records' covers (the server's: the first six records, a crate's own order coming a record
   *  at a time; a first page of 24 tracks was two albums, fewer than the strip's four, and the card showed no covers,
   *  the owner 2026-10-31). One track: the card lists none (the server asks one at least). */
  const COVERS = 6;
  const previewOf = (c: readonly Chip[], m: Modes) =>
    previewCrate(conditionsOf(c, undefined, m), 1, prefs.source || undefined, { without: hiddenLibraries(), covers: COVERS });

  /** The crate shown: its terms, joins and first tracks, together, given only once all are in (the last one stays
   *  while the next words are read, so the card is not gone between keystrokes; the owner's no-flash rule). */
  let shown = $state<Shown | null>(untrack(() => (last?.q === q ? last.shown : null)));
  $effect(() => {
    if (!shown) return;
    crateCards.shown++;
    return () => void crateCards.shown--;
  });
  $effect(() => {
    const s = shown;
    if (s) last = { q: untrack(() => q), shown: s };
  });
  const chips = $derived(shown?.chips ?? null);
  const modes = $derived(shown?.modes ?? {});
  const preview = $derived(shown?.preview ?? null);
  /** Changes made on the card: the newest wins, new words drop them. */
  let changes = 0;
  $effect(() => {
    const c = globalSearch.crate;
    const line = q;
    if (words(line).length < 2 || c === null) return void (shown = null);
    if (c.q !== line) return;
    let gone = false;
    untrack(() => {
      changes++;
      previewOf(c.chips, c.modes).then(
        (p) => !gone && (shown = crateShows(c.chips, p.total) ? { chips: c.chips, modes: c.modes, preview: p } : null),
        () => !gone && (shown = null),
      );
    });
    return () => (gone = true);
  });

  /** A change made on the card (a pill's menu, a word added): the card shows the changed crate once its tracks are in,
   *  the one before until then; a crate left with no terms goes. */
  function change(next: Chip[], m: Modes = modes) {
    const n = ++changes;
    if (!next.length) return void (shown = null);
    previewOf(next, m).then(
      (p) => n === changes && (shown = { chips: next, modes: m, preview: p }),
      (e) => n === changes && toast.fail(e, "It couldn't be changed"),
    );
  }

  const parts = $derived(chips ? sentenceOf(chips, modes) : []);
  /** Its first records' covers, each its first track's (the panel's records' own way, so a crate of any server shows
   *  them). */
  const covers = $derived(preview ? (preview.covers ?? []).map((c) => coverUrl({ source: preview.source, id: c.track, album_id: c.album_id })) : []);
  const likeAt = (i: number | undefined) => i !== undefined && !!chips?.[i] && seedsOf([chips[i]]).length > 0;
  const notAt = (i: number | undefined) => i !== undefined && !!chips?.[i]?.not;

  /** The pill whose menu shows. */
  let at = $state<number | null>(null);
  /** A word being added ("+ word" become its field), and the words that read as nothing. */
  let adding = $state<string | null>(null);
  let missed = $state<string[]>([]);
  async function add(e: KeyboardEvent) {
    if (e.key === 'Escape') return void (adding = null);
    if (e.key !== 'Enter' || !chips || adding === null) return;
    e.preventDefault();
    const line = adding.trim();
    if (!line) return void (adding = null);
    const had = chips;
    const r = readOnto(had, await readLine(line, ask));
    missed = r.missed;
    adding = null;
    if (r.add.length) change([...had, ...r.add], { ...modes, ...r.modes });
  }
  const focusing = (el: HTMLInputElement) => el.focus();

  function grab() {
    if (chips) grabCrate(chips, modes);
  }
  let starting = $state(false);
  async function play() {
    if (!chips) return;
    starting = true;
    try {
      const p = await previewCrate(conditionsOf(chips, undefined, modes), 500, prefs.source || undefined, { without: hiddenLibraries() });
      const ids = p.rows.flatMap((r) => (r.id && !r.missing ? [r.id] : []));
      if (ids.length) await playCollection({ kind: 'tracks', source: p.source, ids }, 'play', nameOf(chips));
    } catch (e) {
      toast.fail(e, "It couldn't be played");
    } finally {
      starting = false;
    }
  }
</script>

{#if shown && chips}
  {@const cs = chips}
  <div class="crate">
    <CoverCard {covers} label="A crate of {q}" onopen={grab} wide>
      <!-- Its terms tapped in the sentence itself, each opening its menu, "+ word" at its end (the owner, 2026-10-31:
           the pills under it said the same sentence again). -->
      <h3 class="sentence">
        {#each parts as p, k (k)}{#if !p.tight && k}{' '}{/if}{#if p.chip !== undefined}{@const i = p.chip}<button class="term" class:like={likeAt(i)} class:not={notAt(i)} aria-label="{cs[i].reading.label}: change or leave out" onclick={() => (at = i)}>{p.text}</button>{:else}<span class="w">{p.text}</span>{/if}{/each}
        {#if adding === null}
          <button class="more" onclick={() => ((adding = ''), (missed = []))}>+ word</button>
        {:else}
          <input class="more" type="search" aria-label="Another word" bind:value={adding} onkeydown={add} onblur={() => !adding?.trim() && (adding = null)} {@attach focusing} />
        {/if}
      </h3>
      {#if missed.length}<p>Nothing found for {missed.join(', ')}</p>{/if}
      <div class="acts">
        <button class="pill play" disabled={starting} onclick={play}><Icon name="play" size={18} />Play</button>
        <button class="pill grab" onclick={grab}>Grab this crate</button>
        <span class="count">{plural(preview?.total ?? 0, 'track')}</span>
      </div>
    </CoverCard>
  </div>
  {#if at !== null && cs[at]}
    {@const i = at}
    <QuickTermSheet
      chip={cs[i]}
      {ask}
      onchange={(c) => change(cs.map((x, k) => (k === i ? c : x)))}
      onremove={() => change(cs.filter((_, k) => k !== i))}
      onclose={() => (at = null)}
    />
  {/if}
{/if}

<style>
  /* Its sentence as the panel writes it (lib/CrateSentence): a term on a line, a Sounds like's in the accent, one left
     out struck through, the words between lighter. */
  .crate {
    margin: 6px 0 10px;
  }

  .sentence {
    line-height: 1.5;
  }

  .term {
    padding: 0;
    border: 0;
    border-bottom: 2px solid var(--border);
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .term:hover,
  .term:focus-visible {
    border-bottom-color: var(--text);
  }

  .term.like {
    border-bottom-color: var(--accent);
  }

  .term.not {
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--danger-ink) 60%, transparent);
  }

  .w {
    color: var(--muted);
    font-weight: 400;
  }

  /* "+ word" at the sentence's end: the week's pill, dashed (another to come). */
  .more {
    margin-left: 4px;
    padding: 2px 10px;
    border: 1px dashed var(--border);
    border-radius: 999px;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 0.85rem;
    font-weight: 400;
    vertical-align: middle;
    cursor: pointer;
  }

  input.more {
    width: 12ch;
    color: var(--text);
    cursor: text;
  }

  .acts {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 12px;
  }

  .count {
    margin-left: auto;
    color: var(--muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  /* Home's two pills: its moment's Play, a quiet one beside it. */
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 18px;
    border: 0;
    border-radius: 999px;
    font: inherit;
    font-size: 14px;
    white-space: nowrap;
    cursor: pointer;
  }

  .pill:disabled {
    opacity: 0.6;
  }

  .play {
    background: var(--accent);
    color: var(--accent-text);
  }

  .grab {
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: inherit;
  }
</style>
